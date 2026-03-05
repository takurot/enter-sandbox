import hashlib
import json
import re
import subprocess
import sys
import tarfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_PATH = REPO_ROOT / "docs" / "PLAN.md"
DOC_PATH = REPO_ROOT / "docs" / "FIRECRACKER_ROOTFS.md"
MANIFEST_PATH = REPO_ROOT / "assets" / "firecracker-rootfs" / "manifest.json"
SCRIPT_PATH = REPO_ROOT / "scripts" / "prepare_firecracker_rootfs.py"


def _sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as file_obj:
        while True:
            chunk = file_obj.read(1024 * 1024)
            if not chunk:
                break
            hasher.update(chunk)
    return hasher.hexdigest()


def _create_rootfs_archive(tmp_path: Path) -> tuple[Path, Path]:
    payload_dir = tmp_path / "payload"
    (payload_dir / "bin").mkdir(parents=True)
    (payload_dir / "etc").mkdir(parents=True)
    (payload_dir / "bin" / "busybox").write_bytes(b"busybox")
    (payload_dir / "etc" / "os-release").write_text("NAME=Alpine\n", encoding="utf-8")

    archive_path = tmp_path / "rootfs.tar.gz"
    with tarfile.open(archive_path, mode="w:gz") as archive:
        for path in sorted(payload_dir.rglob("*")):
            archive.add(path, arcname=str(path.relative_to(payload_dir)))
    return archive_path, payload_dir


def _write_manifest(
    tmp_path: Path, source_archive: Path, payload_dir: Path, size_mb: int = 64
) -> Path:
    manifest = {
        "schema_version": 1,
        "asset": {
            "name": "test-firecracker-rootfs",
            "version": "0.0.0",
            "base_distribution": "alpine",
            "source": {
                "type": "file",
                "url": source_archive.resolve().as_uri(),
            },
            "archive": {
                "file_name": "rootfs.tar.gz",
                "sha256": _sha256(source_archive),
            },
            "extract": {
                "directory": "rootfs",
                "expected_files": [
                    {
                        "path": "bin/busybox",
                        "sha256": _sha256(payload_dir / "bin" / "busybox"),
                    },
                    {
                        "path": "etc/os-release",
                        "sha256": _sha256(payload_dir / "etc" / "os-release"),
                    },
                ],
            },
            "image": {
                "path": "rootfs.ext4",
                "size_mb": size_mb,
                "label": "enter-sandbox",
            },
        },
    }

    manifest_path = tmp_path / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return manifest_path


def test_firecracker_rootfs_doc_has_alpine_workflow_and_exit_criteria():
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "Firecracker Rootfs Image" in text
    assert "Alpine Linux" in text
    assert "rootfs.ext4" in text
    assert "mkfs.ext4" in text
    assert "Linux host" in text
    assert "## Exit Criteria" in text


def test_plan_marks_p2_002_as_completed():
    text = PLAN_PATH.read_text(encoding="utf-8")

    match = re.search(
        r"\|\s*P2-002\s*\|.*?\|\s*`\[x\]`\s*\|",
        text,
    )
    assert match is not None, "P2-002 should be marked as [x] in docs/PLAN.md"


def test_firecracker_rootfs_manifest_is_pinned_and_alpine_based():
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    asset = manifest["asset"]

    assert manifest["schema_version"] == 1
    assert asset["base_distribution"] == "alpine"
    assert asset["source"]["url"].startswith("https://")
    assert asset["archive"]["file_name"].endswith(".tar.gz")
    assert len(asset["archive"]["sha256"]) == 64
    assert asset["image"]["path"] == "rootfs.ext4"
    assert asset["image"]["size_mb"] >= 64


def test_readmes_reference_firecracker_rootfs_doc():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    readme_ja = (REPO_ROOT / "README.ja.md").read_text(encoding="utf-8")

    assert (
        "[Firecracker Rootfs Build (FIRECRACKER_ROOTFS.md)](docs/FIRECRACKER_ROOTFS.md)" in readme
    )
    assert (
        "[Firecracker Rootfs ビルド (FIRECRACKER_ROOTFS.md)](docs/FIRECRACKER_ROOTFS.md)"
        in readme_ja
    )


def test_prepare_firecracker_rootfs_download_extract_and_check(tmp_path: Path):
    source_archive, payload_dir = _create_rootfs_archive(tmp_path)
    manifest_path = _write_manifest(tmp_path, source_archive, payload_dir)
    cache_dir = tmp_path / "downloads"
    extract_dir = tmp_path / "rootfs"
    output_dir = tmp_path / "out"

    command = [
        sys.executable,
        str(SCRIPT_PATH),
        "--manifest",
        str(manifest_path),
        "--cache-dir",
        str(cache_dir),
        "--extract-dir",
        str(extract_dir),
        "--output-dir",
        str(output_dir),
        "--no-build-image",
    ]

    first_run = subprocess.run(command, capture_output=True, text=True)
    assert first_run.returncode == 0, first_run.stderr
    assert (extract_dir / "bin" / "busybox").exists()
    assert (output_dir / "rootfs-image.json").exists()

    check_run = subprocess.run(command + ["--check-only"], capture_output=True, text=True)
    assert check_run.returncode == 0, check_run.stderr


def test_prepare_firecracker_rootfs_check_only_requires_existing_archive(tmp_path: Path):
    source_archive, payload_dir = _create_rootfs_archive(tmp_path)
    manifest_path = _write_manifest(tmp_path, source_archive, payload_dir)
    cache_dir = tmp_path / "missing-downloads"
    extract_dir = tmp_path / "missing-rootfs"
    output_dir = tmp_path / "missing-out"

    command = [
        sys.executable,
        str(SCRIPT_PATH),
        "--manifest",
        str(manifest_path),
        "--cache-dir",
        str(cache_dir),
        "--extract-dir",
        str(extract_dir),
        "--output-dir",
        str(output_dir),
        "--check-only",
        "--no-build-image",
    ]
    run = subprocess.run(command, capture_output=True, text=True)
    assert run.returncode != 0
    assert "Archive not found" in run.stderr


def test_prepare_firecracker_rootfs_rejects_non_positive_image_size(tmp_path: Path):
    source_archive, payload_dir = _create_rootfs_archive(tmp_path)
    manifest_path = _write_manifest(tmp_path, source_archive, payload_dir, size_mb=0)
    cache_dir = tmp_path / "downloads"
    extract_dir = tmp_path / "rootfs"
    output_dir = tmp_path / "out"

    command = [
        sys.executable,
        str(SCRIPT_PATH),
        "--manifest",
        str(manifest_path),
        "--cache-dir",
        str(cache_dir),
        "--extract-dir",
        str(extract_dir),
        "--output-dir",
        str(output_dir),
        "--no-build-image",
    ]
    run = subprocess.run(command, capture_output=True, text=True)
    assert run.returncode != 0
    assert "image.size_mb must be greater than 0" in run.stderr
