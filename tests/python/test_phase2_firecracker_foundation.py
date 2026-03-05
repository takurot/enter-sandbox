import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_PATH = REPO_ROOT / "docs" / "PLAN.md"
DOC_PATH = REPO_ROOT / "docs" / "FIRECRACKER_DEV.md"
DEVCONTAINER_PATH = REPO_ROOT / ".devcontainer" / "devcontainer.json"
VAGRANTFILE_PATH = REPO_ROOT / "Vagrantfile"


def test_firecracker_foundation_doc_has_decision_and_comparison():
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "Firecracker" in text
    assert "libkrun" in text
    assert "## Decision" in text
    assert "Firecracker as the Tier 2 VMM baseline" in text


def test_firecracker_foundation_doc_covers_mac_windows_and_devcontainer():
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "## Development Environments" in text
    assert "### macOS host" in text
    assert "### Windows host" in text
    assert "Vagrant" in text
    assert "Dev Container" in text
    assert "## Exit Criteria" in text


def test_plan_marks_p2_001_as_completed():
    text = PLAN_PATH.read_text(encoding="utf-8")

    match = re.search(
        r"\|\s*P2-001\s*\|.*?\|\s*`\[x\]`\s*\|",
        text,
    )
    assert match is not None, "P2-001 should be marked as [x] in docs/PLAN.md"


def test_readmes_reference_firecracker_dev_doc():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    readme_ja = (REPO_ROOT / "README.ja.md").read_text(encoding="utf-8")

    assert "[Firecracker Dev Environment (FIRECRACKER_DEV.md)](docs/FIRECRACKER_DEV.md)" in readme
    assert "[Firecracker 開発環境 (FIRECRACKER_DEV.md)](docs/FIRECRACKER_DEV.md)" in readme_ja


def test_firecracker_dev_environment_files_exist():
    devcontainer = DEVCONTAINER_PATH.read_text(encoding="utf-8")
    vagrantfile = VAGRANTFILE_PATH.read_text(encoding="utf-8")

    assert '"name": "enter-sandbox-phase2"' in devcontainer
    assert "ghcr.io/devcontainers/base:ubuntu-24.04" in devcontainer
    assert "scripts/prepare_cpython_wasi_assets.py" in devcontainer
    assert 'Vagrant.configure("2")' in vagrantfile
    assert "ubuntu/jammy64" in vagrantfile
    assert 'cd /workspace && python3 scripts/prepare_cpython_wasi_assets.py' in vagrantfile
