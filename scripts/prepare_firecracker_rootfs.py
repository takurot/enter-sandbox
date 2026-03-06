#!/usr/bin/env python3
"""Prepare a pinned Alpine rootfs image input for Firecracker development."""

from __future__ import annotations

import argparse
import hashlib
import inspect
import json
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Tuple


BUFFER_SIZE = 1024 * 1024
DEFAULT_DOWNLOAD_TIMEOUT_SECONDS = 30
DEFAULT_DOWNLOAD_ATTEMPTS = 3


def positive_int(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("must be an integer") from error
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than 0")
    return parsed


def parse_args() -> argparse.Namespace:
    repo_root = Path(__file__).resolve().parents[1]
    default_manifest = repo_root / "assets" / "firecracker-rootfs" / "manifest.json"

    parser = argparse.ArgumentParser(
        description="Download, verify, and prepare pinned Firecracker Alpine rootfs assets."
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=default_manifest,
        help="Path to manifest.json (default: assets/firecracker-rootfs/manifest.json)",
    )
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=None,
        help="Directory for downloaded archives (default: <manifest-dir>/downloads)",
    )
    parser.add_argument(
        "--extract-dir",
        type=Path,
        default=None,
        help="Directory for extracted rootfs (default: from manifest extract.directory)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help="Directory for rootfs image outputs (default: <manifest-dir>/output)",
    )
    parser.add_argument(
        "--check-only",
        action="store_true",
        help="Verify only. Do not download, extract, or build image.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Force re-download/re-extract/rebuild even if outputs already exist.",
    )
    parser.add_argument(
        "--no-build-image",
        action="store_true",
        help="Skip ext4 image build. Useful for macOS/Windows dev flows.",
    )
    parser.add_argument(
        "--download-timeout-seconds",
        type=positive_int,
        default=DEFAULT_DOWNLOAD_TIMEOUT_SECONDS,
        help=(
            "Per-attempt timeout for archive download in seconds (default: {})".format(
                DEFAULT_DOWNLOAD_TIMEOUT_SECONDS
            )
        ),
    )
    parser.add_argument(
        "--download-attempts",
        type=positive_int,
        default=DEFAULT_DOWNLOAD_ATTEMPTS,
        help=("Max attempts for archive download (default: {})".format(DEFAULT_DOWNLOAD_ATTEMPTS)),
    )
    return parser.parse_args()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file_obj:
        while True:
            chunk = file_obj.read(BUFFER_SIZE)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def download_file(url: str, destination: Path, timeout_seconds: int, attempts: int) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "enter-sandbox-p2-002"})

    for attempt_index in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(
                request, timeout=timeout_seconds
            ) as response, destination.open("wb") as out_file:
                while True:
                    chunk = response.read(BUFFER_SIZE)
                    if not chunk:
                        break
                    out_file.write(chunk)
            return
        except (OSError, TimeoutError, urllib.error.URLError) as error:
            destination.unlink(missing_ok=True)
            if attempt_index >= attempts:
                raise RuntimeError(
                    "Failed to download {} after {} attempts: {}".format(url, attempts, error)
                ) from error
            print(
                "[prepare-firecracker-rootfs] download attempt {}/{} failed: {} (retrying)".format(
                    attempt_index, attempts, error
                )
            )
            time.sleep(min(2.0, attempt_index * 0.2))


def verify_archive(archive_path: Path, expected_hash: str, expected_size: Optional[int]) -> None:
    if expected_size is not None:
        actual_size = archive_path.stat().st_size
        if actual_size != expected_size:
            raise RuntimeError(
                "Archive size mismatch: expected={}, actual={}".format(expected_size, actual_size)
            )

    actual_hash = sha256_file(archive_path)
    if actual_hash != expected_hash:
        raise RuntimeError(
            "Archive SHA256 mismatch: expected={}, actual={}".format(expected_hash, actual_hash)
        )


def ensure_safe_tar_members(members: Iterable[tarfile.TarInfo]) -> None:
    for member in members:
        member_path = Path(member.name)
        if member_path.is_absolute() or ".." in member_path.parts:
            raise RuntimeError("Tar contains unsafe path: {}".format(member.name))


def extract_archive(archive_path: Path, extract_dir: Path) -> None:
    if extract_dir.exists():
        shutil.rmtree(extract_dir)
    extract_dir.mkdir(parents=True, exist_ok=True)

    with tarfile.open(archive_path, "r:gz") as tar_file:
        members = tar_file.getmembers()
        ensure_safe_tar_members(members)
        extract_kwargs = {}
        if "filter" in inspect.signature(tarfile.TarFile.extractall).parameters:
            # Python 3.12+ defaults to restrictive extraction filters that reject
            # valid rootfs absolute symlink targets (for example: /bin/busybox).
            extract_kwargs["filter"] = "fully_trusted"
        tar_file.extractall(extract_dir, **extract_kwargs)


def verify_expected_files(extract_dir: Path, expected_files: List[Dict[str, str]]) -> None:
    for expected in expected_files:
        relative_path = expected["path"]
        expected_hash = expected["sha256"]
        target = extract_dir / relative_path
        if not target.exists():
            raise RuntimeError("Expected extracted file is missing: {}".format(target))
        actual_hash = sha256_file(target)
        if actual_hash != expected_hash:
            raise RuntimeError(
                "SHA256 mismatch for {}: expected={}, actual={}".format(
                    target, expected_hash, actual_hash
                )
            )


def load_manifest(manifest_path: Path) -> Dict[str, object]:
    with manifest_path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)

    try:
        asset = data["asset"]
        _ = asset["source"]["url"]
        _ = asset["archive"]["file_name"]
        _ = asset["archive"]["sha256"]
        _ = asset["extract"]["directory"]
        _ = asset["extract"]["expected_files"]
        image_size_mb = asset["image"]["size_mb"]
    except (KeyError, TypeError) as error:
        raise RuntimeError("manifest is missing required keys") from error

    if not isinstance(image_size_mb, int) or image_size_mb <= 0:
        raise RuntimeError("image.size_mb must be greater than 0")

    return data


def ensure_archive(
    archive_path: Path,
    source_url: str,
    expected_hash: str,
    expected_size: Optional[int],
    check_only: bool,
    timeout_seconds: int,
    attempts: int,
) -> None:
    if not archive_path.exists():
        if check_only:
            raise RuntimeError("Archive not found in --check-only mode: {}".format(archive_path))
        print("[prepare-firecracker-rootfs] downloading {}".format(source_url))
        download_file(source_url, archive_path, timeout_seconds, attempts)

    try:
        verify_archive(archive_path, expected_hash, expected_size)
        return
    except RuntimeError as error:
        if check_only:
            raise
        print(
            "[prepare-firecracker-rootfs] cached archive mismatch: {} (re-downloading)".format(
                error
            )
        )
        archive_path.unlink(missing_ok=True)
        download_file(source_url, archive_path, timeout_seconds, attempts)

    try:
        verify_archive(archive_path, expected_hash, expected_size)
    except RuntimeError as error:
        archive_path.unlink(missing_ok=True)
        raise RuntimeError(
            "Archive verification failed after re-download. Removed {}: {}".format(
                archive_path, error
            )
        ) from error


def resolve_mkfs_command() -> Tuple[List[str], str]:
    mkfs_ext4 = shutil.which("mkfs.ext4")
    if mkfs_ext4 is not None:
        return [mkfs_ext4], "mkfs.ext4"

    mke2fs = shutil.which("mke2fs")
    if mke2fs is not None:
        return [mke2fs, "-t", "ext4"], "mke2fs"

    raise RuntimeError(
        "mkfs.ext4 (or mke2fs) is required to build rootfs.ext4. "
        "Install e2fsprogs or run with --no-build-image."
    )


def build_rootfs_image(
    extract_dir: Path, image_path: Path, image_size_mb: int, image_label: str
) -> str:
    image_path.parent.mkdir(parents=True, exist_ok=True)
    image_path.unlink(missing_ok=True)

    with image_path.open("wb") as image_file:
        image_file.truncate(image_size_mb * 1024 * 1024)

    command_prefix, builder_name = resolve_mkfs_command()
    command = [
        *command_prefix,
        "-F",
        "-L",
        image_label,
        "-d",
        str(extract_dir),
        str(image_path),
    ]

    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        image_path.unlink(missing_ok=True)
        raise RuntimeError(
            "Failed to build rootfs image with {}: {} {}".format(
                builder_name, result.stdout.strip(), result.stderr.strip()
            ).strip()
        )

    return builder_name


def write_image_metadata(
    metadata_path: Path,
    manifest_path: Path,
    archive_path: Path,
    extract_dir: Path,
    image_path: Path,
    image_size_mb: int,
    image_label: str,
    image_built: bool,
    image_builder: str,
) -> None:
    metadata_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema_version": 1,
        "image": {
            "path": str(image_path),
            "size_mb": image_size_mb,
            "label": image_label,
            "built": image_built,
            "builder": image_builder,
        },
        "inputs": {
            "manifest": str(manifest_path),
            "archive": str(archive_path),
            "extract_dir": str(extract_dir),
        },
    }
    metadata_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def load_image_metadata(metadata_path: Path) -> Dict[str, object]:
    try:
        payload = json.loads(metadata_path.read_text(encoding="utf-8"))
    except OSError as error:
        raise RuntimeError(
            "Failed to read rootfs image metadata {}: {}".format(metadata_path, error)
        ) from error
    except json.JSONDecodeError as error:
        raise RuntimeError(
            "Failed to parse rootfs image metadata {}: {}".format(metadata_path, error)
        ) from error

    if not isinstance(payload, dict):
        raise RuntimeError("Rootfs image metadata must be a JSON object: {}".format(metadata_path))
    return payload


def validate_image_metadata(
    metadata_path: Path,
    metadata: Dict[str, object],
    manifest_path: Path,
    archive_path: Path,
    extract_dir: Path,
    image_path: Path,
    image_size_mb: int,
    image_label: str,
) -> str:
    try:
        schema_version = metadata["schema_version"]
        image = metadata["image"]
        inputs = metadata["inputs"]
        metadata_image_path = image["path"]
        metadata_size_mb = image["size_mb"]
        metadata_label = image["label"]
        metadata_built = image["built"]
        metadata_builder = image["builder"]
        metadata_manifest = inputs["manifest"]
        metadata_archive = inputs["archive"]
        metadata_extract_dir = inputs["extract_dir"]
    except (KeyError, TypeError) as error:
        raise RuntimeError(
            "Rootfs image metadata is missing required keys: {}".format(metadata_path)
        ) from error

    if schema_version != 1:
        raise RuntimeError(
            "Rootfs image metadata schema_version must be 1: {}".format(metadata_path)
        )

    expected_fields = (
        ("image.path", metadata_image_path, str(image_path)),
        ("image.size_mb", metadata_size_mb, image_size_mb),
        ("image.label", metadata_label, image_label),
        ("inputs.manifest", metadata_manifest, str(manifest_path)),
        ("inputs.archive", metadata_archive, str(archive_path)),
        ("inputs.extract_dir", metadata_extract_dir, str(extract_dir)),
    )
    for field_name, actual, expected in expected_fields:
        if actual != expected:
            raise RuntimeError(
                "Rootfs image metadata mismatch for {}: expected={}, actual={}".format(
                    field_name, expected, actual
                )
            )

    if not isinstance(metadata_built, bool):
        raise RuntimeError(
            "Rootfs image metadata has invalid image.built value: {}".format(metadata_path)
        )
    if not isinstance(metadata_builder, str) or not metadata_builder:
        raise RuntimeError(
            "Rootfs image metadata has invalid image.builder value: {}".format(metadata_path)
        )

    return metadata_builder


def prepare_rootfs(args: argparse.Namespace) -> None:
    manifest_path = args.manifest.resolve()
    manifest_dir = manifest_path.parent
    manifest = load_manifest(manifest_path)
    asset = manifest["asset"]
    source = asset["source"]
    archive = asset["archive"]
    extract = asset["extract"]
    image = asset["image"]

    archive_name = archive["file_name"]
    archive_sha256 = archive["sha256"]
    archive_size_bytes = archive.get("size_bytes")
    source_url = source["url"]
    extract_subdir = extract["directory"]
    expected_files = extract["expected_files"]
    image_relative_path = image["path"]
    image_size_mb = image["size_mb"]
    image_label = image["label"]

    cache_dir = (args.cache_dir or (manifest_dir / "downloads")).resolve()
    extract_dir = (args.extract_dir or (manifest_dir / extract_subdir)).resolve()
    output_dir = (args.output_dir or (manifest_dir / "output")).resolve()
    archive_path = cache_dir / archive_name
    image_path = output_dir / image_relative_path
    image_metadata_path = output_dir / "rootfs-image.json"

    if args.force and not args.check_only:
        archive_path.unlink(missing_ok=True)
        if extract_dir.exists():
            shutil.rmtree(extract_dir)
        image_path.unlink(missing_ok=True)

    ensure_archive(
        archive_path=archive_path,
        source_url=source_url,
        expected_hash=archive_sha256,
        expected_size=archive_size_bytes,
        check_only=args.check_only,
        timeout_seconds=args.download_timeout_seconds,
        attempts=args.download_attempts,
    )

    extracted_rootfs_changed = False
    should_extract = args.force or not extract_dir.exists()
    if should_extract:
        if args.check_only:
            raise RuntimeError(
                "Extracted rootfs not found in --check-only mode: {}".format(extract_dir)
            )
        print("[prepare-firecracker-rootfs] extracting to {}".format(extract_dir))
        extract_archive(archive_path, extract_dir)
        extracted_rootfs_changed = True

    try:
        verify_expected_files(extract_dir, expected_files)
    except RuntimeError:
        if args.check_only:
            raise
        print("[prepare-firecracker-rootfs] extracted files mismatch, re-extracting...")
        extract_archive(archive_path, extract_dir)
        verify_expected_files(extract_dir, expected_files)
        extracted_rootfs_changed = True

    metadata_builder: Optional[str] = None
    metadata_valid = False
    if image_metadata_path.exists():
        try:
            metadata_builder = validate_image_metadata(
                metadata_path=image_metadata_path,
                metadata=load_image_metadata(image_metadata_path),
                manifest_path=manifest_path,
                archive_path=archive_path,
                extract_dir=extract_dir,
                image_path=image_path,
                image_size_mb=image_size_mb,
                image_label=image_label,
            )
            metadata_valid = True
        except RuntimeError as error:
            if args.check_only:
                raise
            print(
                "[prepare-firecracker-rootfs] existing metadata mismatch: {} (refreshing)".format(
                    error
                )
            )
    elif args.check_only:
        raise RuntimeError(
            "Rootfs image metadata not found in --check-only mode: {}".format(image_metadata_path)
        )

    image_builder = "skipped"
    image_built = False
    if metadata_valid and image_path.exists():
        image_builder = metadata_builder or "existing"
        image_built = True

    if args.no_build_image:
        pass
    else:
        if args.check_only:
            if not image_path.exists():
                raise RuntimeError(
                    "Rootfs image not found in --check-only mode: {}".format(image_path)
                )
            image_builder = metadata_builder or "existing"
            image_built = True
        else:
            should_build_image = (
                args.force
                or extracted_rootfs_changed
                or not image_path.exists()
                or not metadata_valid
            )
            if should_build_image:
                print("[prepare-firecracker-rootfs] building image {}".format(image_path))
                image_builder = build_rootfs_image(
                    extract_dir, image_path, image_size_mb, image_label
                )
                image_built = True
            else:
                image_builder = metadata_builder or "existing"
                image_built = True

    if not args.check_only:
        write_image_metadata(
            metadata_path=image_metadata_path,
            manifest_path=manifest_path,
            archive_path=archive_path,
            extract_dir=extract_dir,
            image_path=image_path,
            image_size_mb=image_size_mb,
            image_label=image_label,
            image_built=image_built,
            image_builder=image_builder,
        )

    print("[prepare-firecracker-rootfs] manifest={}".format(manifest_path))
    print("[prepare-firecracker-rootfs] archive={}".format(archive_path))
    print("[prepare-firecracker-rootfs] rootfs={}".format(extract_dir))
    print("[prepare-firecracker-rootfs] metadata={}".format(image_metadata_path))
    print("[prepare-firecracker-rootfs] image={}".format(image_path))
    print("[prepare-firecracker-rootfs] status=ok")


def main() -> int:
    args = parse_args()
    try:
        prepare_rootfs(args)
    except Exception as exc:  # noqa: BLE001
        print("[prepare-firecracker-rootfs] status=error message={}".format(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
