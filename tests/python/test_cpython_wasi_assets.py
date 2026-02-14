import hashlib
import json
import subprocess
import sys
import zipfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "scripts" / "prepare_cpython_wasi_assets.py"


def _sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as file_obj:
        while True:
            chunk = file_obj.read(1024 * 1024)
            if not chunk:
                break
            hasher.update(chunk)
    return hasher.hexdigest()


def _create_payload_archive(tmp_path: Path) -> Path:
    payload_dir = tmp_path / "payload"
    (payload_dir / "lib/python3.13/json").mkdir(parents=True)
    (payload_dir / "python.wasm").write_bytes(b"wasm")
    (payload_dir / "lib/python3.13/json/__init__.py").write_text(
        "print('json')\n", encoding="utf-8"
    )

    archive_path = tmp_path / "source.zip"
    with zipfile.ZipFile(archive_path, mode="w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(payload_dir.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(payload_dir))
    return archive_path


def _write_manifest(tmp_path: Path, source_archive: Path) -> Path:
    extracted_payload = tmp_path / "payload"
    expected_files = [
        {
            "path": "python.wasm",
            "sha256": _sha256(extracted_payload / "python.wasm"),
        },
        {
            "path": "lib/python3.13/json/__init__.py",
            "sha256": _sha256(extracted_payload / "lib/python3.13/json/__init__.py"),
        },
    ]

    manifest = {
        "schema_version": 1,
        "asset": {
            "name": "test-runtime",
            "version": "0.0.0",
            "source": {
                "type": "file",
                "url": source_archive.resolve().as_uri(),
            },
            "archive": {
                "file_name": "test-runtime.zip",
                "sha256": _sha256(source_archive),
            },
            "extract": {
                "directory": "runtime",
                "expected_files": expected_files,
            },
        },
    }

    manifest_path = tmp_path / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return manifest_path


def test_prepare_cpython_wasi_assets_download_extract_and_check(tmp_path: Path):
    source_archive = _create_payload_archive(tmp_path)
    manifest_path = _write_manifest(tmp_path, source_archive)
    cache_dir = tmp_path / "downloads"
    extract_dir = tmp_path / "runtime"

    command = [
        sys.executable,
        str(SCRIPT_PATH),
        "--manifest",
        str(manifest_path),
        "--cache-dir",
        str(cache_dir),
        "--extract-dir",
        str(extract_dir),
    ]
    first_run = subprocess.run(command, capture_output=True, text=True)
    assert first_run.returncode == 0, first_run.stderr
    assert (extract_dir / "python.wasm").exists()

    check_run = subprocess.run(command + ["--check-only"], capture_output=True, text=True)
    assert check_run.returncode == 0, check_run.stderr


def test_prepare_cpython_wasi_assets_check_only_requires_existing_archive(tmp_path: Path):
    source_archive = _create_payload_archive(tmp_path)
    manifest_path = _write_manifest(tmp_path, source_archive)
    cache_dir = tmp_path / "missing-downloads"
    extract_dir = tmp_path / "missing-runtime"

    command = [
        sys.executable,
        str(SCRIPT_PATH),
        "--manifest",
        str(manifest_path),
        "--cache-dir",
        str(cache_dir),
        "--extract-dir",
        str(extract_dir),
        "--check-only",
    ]
    run = subprocess.run(command, capture_output=True, text=True)
    assert run.returncode != 0
    assert "Archive not found" in run.stderr
