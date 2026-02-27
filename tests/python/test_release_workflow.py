from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "release.yml"


def _workflow_text() -> str:
    return WORKFLOW_PATH.read_text(encoding="utf-8")


def test_release_workflow_has_expected_triggers():
    text = _workflow_text()
    assert "release:" in text
    assert "types: [published]" in text
    assert "workflow_dispatch:" in text


def test_release_workflow_builds_and_publishes_to_pypi():
    text = _workflow_text()
    assert "name: Build distributions" in text
    assert "python scripts/build_pypi_artifacts.py --out-dir dist" in text
    assert "uses: actions/upload-artifact@v4" in text
    assert "name: Publish to PyPI" in text
    assert "id-token: write" in text
    assert "uses: pypa/gh-action-pypi-publish@release/v1" in text
