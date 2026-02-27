# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project follows Semantic Versioning.

## [Unreleased]

### Added

- Added GitHub Release -> PyPI publishing workflow with OIDC (P1-061).
- Added SemVer operation rules and release checklist documentation (P1-062).

## [0.1.0] - 2026-02-27

### Added

- Initial Phase 1 alpha baseline:
  - Wasm runner integration and Python SDK bridge (`Sandbox`, `SandboxConfig`, `SandboxResult`)
  - CI checks for Rust/Python quality gates
  - CPython WASI repro harness and regression tests
  - PyPI packaging script and metadata setup

[Unreleased]: https://github.com/takurot/enter-sandbox/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/takurot/enter-sandbox/releases/tag/v0.1.0
