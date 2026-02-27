import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "scripts" / "build_pypi_artifacts.py"


def test_build_pypi_artifacts_dry_run_builds_wheel_and_sdist(tmp_path: Path):
    out_dir = tmp_path / "dist"
    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--dry-run",
            "--out-dir",
            str(out_dir),
            "--manifest-path",
            "agentbox-core/Cargo.toml",
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 0, run.stdout + run.stderr
    assert "Using PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1" in run.stdout
    assert "Dry run enabled; no artifacts will be built." in run.stdout
    assert "maturin build --release" in run.stdout
    assert "maturin sdist" in run.stdout


def test_build_pypi_artifacts_dry_run_skip_sdist(tmp_path: Path):
    out_dir = tmp_path / "dist"
    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--dry-run",
            "--skip-sdist",
            "--out-dir",
            str(out_dir),
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 0, run.stdout + run.stderr
    assert "maturin build --release" in run.stdout
    assert "maturin sdist" not in run.stdout


def test_build_pypi_artifacts_rejects_skip_all():
    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--skip-wheel",
            "--skip-sdist",
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 2
    assert "At least one artifact type must be enabled." in run.stderr


def test_python_metadata_matches_abi3_baseline():
    pyproject_text = (REPO_ROOT / "pyproject.toml").read_text(encoding="utf-8")
    cargo_text = (REPO_ROOT / "agentbox-core" / "Cargo.toml").read_text(encoding="utf-8")

    assert "abi3-py310" in cargo_text
    assert 'requires-python = ">=3.10"' in pyproject_text
    assert '"Programming Language :: Python :: 3.10"' in pyproject_text
    assert '"Programming Language :: Python :: 3.8"' not in pyproject_text
    assert '"Programming Language :: Python :: 3.9"' not in pyproject_text
