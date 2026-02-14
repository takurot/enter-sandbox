#!/usr/bin/env python3
"""Prepare and verify pinned CPython WASI assets for reproducible runs."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import urllib.request
import zipfile
from pathlib import Path
from typing import Dict, Iterable, List

BUFFER_SIZE = 1024 * 1024


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


def download_file(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "enter-sandbox-p1-070"})

    with urllib.request.urlopen(request) as response, destination.open("wb") as out_file:
        while True:
            chunk = response.read(BUFFER_SIZE)
            if not chunk:
                break
            out_file.write(chunk)


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
    source_url = source["url"]
    extract_subdir = extract["directory"]
    expected_files = extract["expected_files"]

    cache_dir = (args.cache_dir or (manifest_dir / "downloads")).resolve()
    extract_dir = (args.extract_dir or (manifest_dir / extract_subdir)).resolve()
    archive_path = cache_dir / archive_name

    if args.force and archive_path.exists() and not args.check_only:
        archive_path.unlink()

    if not archive_path.exists():
        if args.check_only:
            raise RuntimeError("Archive not found in --check-only mode: {}".format(archive_path))
        print("[prepare-cpython-wasi] downloading {}".format(source_url))
        download_file(source_url, archive_path)

    actual_archive_hash = sha256_file(archive_path)
    if actual_archive_hash != archive_sha256:
        if args.check_only:
            raise RuntimeError(
                "Archive SHA256 mismatch: expected={}, actual={}".format(
                    archive_sha256, actual_archive_hash
                )
            )
        archive_path.unlink(missing_ok=True)
        raise RuntimeError(
            "Archive hash mismatch. Removed {} to avoid non-deterministic input.".format(
                archive_path
            )
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
