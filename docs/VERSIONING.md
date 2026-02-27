# Versioning Strategy

This document defines how EnterSandBox versions releases and maintains changelog history.

## 1. Scope

- Applies to the published Python package `agentbox`.
- Applies to the Rust crate `agentbox-core` used by maturin.

## 2. SemVer Policy

EnterSandBox follows Semantic Versioning: `MAJOR.MINOR.PATCH`.

- `PATCH`: backward-compatible bug fix or non-breaking internal change.
- `MINOR`: backward-compatible feature addition.
- `MAJOR`: backward-incompatible API/behavior change.

Pre-release identifiers are allowed (`-rc.1`, `-beta.1`) when needed for release candidates.

## 3. Version Source of Truth

Release versions must stay in sync across:

- `pyproject.toml` -> `[project].version`
- `agentbox-core/Cargo.toml` -> `[package].version`

Do not publish if these versions diverge.

## 4. Tag and Release Contract

- Git tags use `vX.Y.Z` format (example: `v0.2.0`).
- A GitHub Release must be created from that tag.
- Publishing the Release (`published`) triggers `.github/workflows/release.yml`, which uploads artifacts to PyPI.

## 5. Changelog Policy

- `CHANGELOG.md` must include:
  - `## [Unreleased]` for upcoming changes
  - one section for each released version (`## [X.Y.Z] - YYYY-MM-DD`)
- Every user-visible change merges with a changelog update in the same PR.
- Release PRs move items from `Unreleased` to the new version section.

## 6. Release Checklist

1. Confirm all release-targeted changes are listed under `Unreleased` in `CHANGELOG.md`.
2. Bump versions in `pyproject.toml` and `agentbox-core/Cargo.toml` to the same `X.Y.Z`.
3. Rename `Unreleased` entries into `## [X.Y.Z] - YYYY-MM-DD` and recreate an empty `Unreleased` section.
4. Run quality gates:
   - `ruff check python/ scripts/ tests/`
   - `ruff format python/ scripts/ tests/ --check`
   - `pytest tests/python`
   - `cargo test --manifest-path agentbox-core/Cargo.toml`
   - `cargo clippy --manifest-path agentbox-core/Cargo.toml -- -D warnings`
5. Create release commit (example): `chore(release): vX.Y.Z`.
6. Tag and push:
   - `git tag vX.Y.Z`
   - `git push origin vX.Y.Z`
7. Create/publish GitHub Release from `vX.Y.Z`.
