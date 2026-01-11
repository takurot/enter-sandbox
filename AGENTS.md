# Repository Guidelines

## Project Structure & Module Organization
- `agentbox-core/`: Rust core (Wasmtime/WASI, PyO3 bindings).
- `python/agentbox/`: Python package exposing `Sandbox`, `SandboxConfig`.
- `runner-wasm/`: Wasm runner binary used by the core.
- `tests/python/`: Pytest suite for the Python API and bindings.
- `docs/`: Specs and plans; reference for architecture and roadmap.

## Build, Test, and Development Commands
- Setup (recommended):
  - `python3 -m venv .venv && source .venv/bin/activate`
  - `pip install -r requirements-dev.txt`
- Build Python extension (Rust core via maturin): `maturin develop`
- Python tests: `pytest` (configured to `tests/python` in `pyproject.toml`)
- Rust (core): `cd agentbox-core && cargo build && cargo test`
- Rust (wasm runner): `cd runner-wasm && cargo build --release --target wasm32-wasip1`
- Lint/format: `ruff check . && ruff format .`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all`

## Coding Style & Naming Conventions
- Python: 4-space indent, max line length 100 (`ruff`), target `py38`. Use type hints and docstrings for public APIs.
  - Names: modules/functions `snake_case`, classes `CamelCase`, constants `UPPER_SNAKE_CASE`.
- Rust: follow `rustfmt` defaults; prefer `?` over `unwrap()` in non-test code.
  - Names: crates/modules `snake_case`, types/traits `CamelCase`, functions/vars `snake_case`.

## Testing Guidelines
- Frameworks: Pytest for Python, Cargo tests for Rust.
- Python tests live in `tests/python/`; files `test_*.py`, functions `test_*`.
- Add tests with new behavior; avoid lowering coverage. Run `pytest -q` and `cargo test` before pushing.

## Commit & Pull Request Guidelines
- Commit messages: follow Conventional Commits where practical (e.g., `feat(core): add VFS snapshot`, `ci: update toolchain`).
- PRs must include: clear description (what/why), linked issues, test coverage for changes, and any relevant docs updates.
- CI hygiene: code builds with `maturin develop`; `pytest`, `cargo test`, `ruff`, `cargo clippy` all pass locally.

## Architecture Overview
Hybrid runtime: Python API wraps a Rust core that loads a Wasm runner. The router/runtime focus on speed and governance; see `docs/SPEC.md` and `docs/PLAN.md` for details.

