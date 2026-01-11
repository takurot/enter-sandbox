# Implementation Playbook (AI Agent)

Use this file as the default execution rules for implementing tasks/PRs in this repository.

## Inputs (What you are given)
- A PR identifier (e.g., **P1-001**) or a request to implement a subset of tasks.

## Primary References (Always read these first)
- **Specification**: `docs/SPEC.md`
- **Implementation tasks**: `docs/PLAN.md`
- **This playbook (process & rules)**: `docs/PROMPT.md`

---

## Non‑Negotiable Project Principles

### 1) Strict Isolation & Security
- **No `exec/eval`**: Never execute user code directly in the host process.
- **Physical Boundary**: All execution must occur inside Wasm (Tier 1) or MicroVM (Tier 2).
- **Capability-based Security**: Only explicitly granted capabilities (WASI) should be available.

### 2) Resource Control
- Every execution **MUST** have a memory limit and a CPU/Time limit (timeout).
- Tier 1 uses Wasmtime resource limiters. Tier 2 uses Firecracker cgroups/quotas.

### 3) Determinism & Reproducibility
- Given the same code, environment, and seed, the execution result (and side effects in VFS) should be deterministic.
- Use a virtual file system (VFS) to isolate file I/O.

### 4) Performance & Efficiency
- Tier 1 (Nano-Sandbox) targets <10ms startup. Optimize RustPython and Wasmtime initialization.
- Tier 2 (Heavy-Sandbox) targets <150ms startup using snapshots.

### 5) Layered Testing (TDD)
- **Rust Unit Tests**: For core logic (Engine, VFS, Governance).
- **Python Integration Tests**: For the `agentbox` SDK and E2E execution scenarios.

---

## Standard Implementation Workflow

### 0) Pre‑flight
- Read `docs/SPEC.md` and the relevant section(s) in `docs/PLAN.md`.
- Create a new branch: `feature/<task-id>-<short-description>`.
- Create a short checklist in your current task summary.

### 1) Environment Setup (macOS/Linux)
```bash
# Python
python3 -m venv .venv
source .venv/bin/activate
pip install -U pip maturin ruff pytest

# Rust
# Ensure rustup is installed with wasm32-wasip1 target if needed
rustup target add wasm32-wasip1
```

### 2) TDD (Test‑Driven Development)
- **Red**: Write a failing test in `tests/` or `src/lib.rs`.
- **Green**: Implement the minimum code to pass.
- **Refactor**: Clean up and optimize.

### 3) Commands
- **Rust**: `cargo build`, `cargo test`, `cargo clippy`.
- **Python-Rust bridge**: `maturin develop` inside `.venv`.
- **Python Tests**: `pytest`.

### 4) Documentation & Updates
- Update `docs/PLAN.md` status.
- Add notes to `docs/PLAN.md` regarding any architectural decisions made.

---

## Checklist (Before you call a task “done”)
- [ ] Code follows the principles (Isolation, Resource Control).
- [ ] Tests covering both happy path and edge cases (limits, timeouts).
- [ ] `cargo clippy` and `ruff` are clean.
- [ ] Documentation updated if API changed.
- [ ] `docs/PLAN.md` status updated.
