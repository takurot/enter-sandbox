#!/usr/bin/env python3
"""Prepare and verify pinned CPython WASI assets for reproducible runs."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Dict, Iterable, List, Optional

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
    default_manifest = repo_root / "assets" / "cpython-wasi" / "manifest.json"

    parser = argparse.ArgumentParser(
        description="Download and verify CPython WASI assets pinned by manifest."
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=default_manifest,
        help="Path to manifest.json (default: assets/cpython-wasi/manifest.json)",
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
        help="Directory for extracted runtime (default: from manifest extract.directory)",
    )
    parser.add_argument(
        "--check-only",
        action="store_true",
        help="Verify only. Do not download or extract.",
    )
    parser.add_argument(
        "--no-extract",
        action="store_true",
        help="Skip extraction and extracted-file verification.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Force re-download/re-extract even if files already exist.",
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
    request = urllib.request.Request(url, headers={"User-Agent": "enter-sandbox-p1-070"})

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
                "[prepare-cpython-wasi] download attempt {}/{} failed: {} (retrying)".format(
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


def ensure_safe_members(members: Iterable[zipfile.ZipInfo]) -> None:
    for member in members:
        member_path = Path(member.filename)
        if member_path.is_absolute() or ".." in member_path.parts:
            raise RuntimeError("Zip contains unsafe path: {}".format(member.filename))


def extract_archive(archive_path: Path, extract_dir: Path) -> None:
    if extract_dir.exists():
        shutil.rmtree(extract_dir)
    extract_dir.mkdir(parents=True, exist_ok=True)

    with zipfile.ZipFile(archive_path) as zip_file:
        ensure_safe_members(zip_file.infolist())
        zip_file.extractall(extract_dir)


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
        return json.load(handle)


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
        print("[prepare-cpython-wasi] downloading {}".format(source_url))
        download_file(source_url, archive_path, timeout_seconds, attempts)

    try:
        verify_archive(archive_path, expected_hash, expected_size)
        return
    except RuntimeError as error:
        if check_only:
            raise

        print("[prepare-cpython-wasi] cached archive mismatch: {} (re-downloading)".format(error))
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


def prepare_assets(args: argparse.Namespace) -> None:
    manifest_path = args.manifest.resolve()
    manifest_dir = manifest_path.parent
    manifest = load_manifest(manifest_path)
    asset = manifest["asset"]
    source = asset["source"]
    archive = asset["archive"]
    extract = asset["extract"]

    archive_name = archive["file_name"]
    archive_sha256 = archive["sha256"]
    archive_size_bytes = archive.get("size_bytes")
    source_url = source["url"]
    extract_subdir = extract["directory"]
    expected_files = extract["expected_files"]

    cache_dir = (args.cache_dir or (manifest_dir / "downloads")).resolve()
    extract_dir = (args.extract_dir or (manifest_dir / extract_subdir)).resolve()
    archive_path = cache_dir / archive_name

    if args.force and archive_path.exists() and not args.check_only:
        archive_path.unlink()

    ensure_archive(
        archive_path=archive_path,
        source_url=source_url,
        expected_hash=archive_sha256,
        expected_size=archive_size_bytes,
        check_only=args.check_only,
        timeout_seconds=args.download_timeout_seconds,
        attempts=args.download_attempts,
    )

    if not args.no_extract:
        should_extract = args.force or not extract_dir.exists()
        if should_extract:
            if args.check_only:
                raise RuntimeError(
                    "Extracted runtime not found in --check-only mode: {}".format(extract_dir)
                )
            print("[prepare-cpython-wasi] extracting to {}".format(extract_dir))
            extract_archive(archive_path, extract_dir)

        try:
            verify_expected_files(extract_dir, expected_files)
        except RuntimeError:
            if args.check_only:
                raise
            print("[prepare-cpython-wasi] extracted files mismatch, re-extracting...")
            extract_archive(archive_path, extract_dir)
            verify_expected_files(extract_dir, expected_files)

    print("[prepare-cpython-wasi] manifest={}".format(manifest_path))
    print("[prepare-cpython-wasi] archive={}".format(archive_path))
    if not args.no_extract:
        print("[prepare-cpython-wasi] runtime={}".format(extract_dir))
    print("[prepare-cpython-wasi] status=ok")


def main() -> int:
    args = parse_args()
    try:
        prepare_assets(args)
    except Exception as exc:  # noqa: BLE001
        print("[prepare-cpython-wasi] status=error message={}".format(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
