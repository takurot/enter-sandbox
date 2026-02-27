import re
import tomllib
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SEMVER_PATTERN = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


def _python_package_version() -> str:
    pyproject = tomllib.loads((REPO_ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    return pyproject["project"]["version"]


def _cargo_package_version() -> str:
    cargo_text = (REPO_ROOT / "agentbox-core" / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo_text, re.MULTILINE)
    assert match is not None, "Cargo package version must be declared."
    return match.group(1)


def _runner_wasm_version() -> str:
    cargo_text = (REPO_ROOT / "runner-wasm" / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo_text, re.MULTILINE)
    assert match is not None, "Runner wasm version must be declared."
    return match.group(1)


def test_versions_are_semver_and_in_sync():
    pyproject_version = _python_package_version()
    cargo_version = _cargo_package_version()
    runner_wasm_version = _runner_wasm_version()

    assert SEMVER_PATTERN.fullmatch(pyproject_version), pyproject_version
    assert SEMVER_PATTERN.fullmatch(cargo_version), cargo_version
    assert SEMVER_PATTERN.fullmatch(runner_wasm_version), runner_wasm_version
    assert pyproject_version == cargo_version == runner_wasm_version


def test_changelog_contains_unreleased_and_current_release_section():
    current_version = _python_package_version()
    changelog = (REPO_ROOT / "CHANGELOG.md").read_text(encoding="utf-8")

    assert "## [Unreleased]" in changelog
    assert f"## [{current_version}] - " in changelog
    assert "[Unreleased]: https://github.com/takurot/enter-sandbox/compare/" in changelog


def test_readme_references_versioning_docs():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    readme_ja = (REPO_ROOT / "README.ja.md").read_text(encoding="utf-8")

    assert "[Versioning Strategy (VERSIONING.md)](docs/VERSIONING.md)" in readme
    assert "[Changelog](CHANGELOG.md)" in readme
    assert "[バージョニング戦略 (VERSIONING.md)](docs/VERSIONING.md)" in readme_ja
    assert "[変更履歴 (CHANGELOG.md)](CHANGELOG.md)" in readme_ja
