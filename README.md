# ⏳ EnterSandBox

**Governance-First AI Agent Runtime Platform**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status: Planning](https://img.shields.io/badge/Status-Planning-yellow)](docs/PLAN.md)
[🇯🇵 日本語 (Japanese)](README.ja.md)

EnterSandBox is a next-generation code execution platform focused on **Governance** and **Observability** for autonomous AI agents.
More than just a fast code execution environment (Runner), it achieves both "Speed" and "Compatibility" using hybrid runtime technology, while providing powerful control functions to meet enterprise security requirements.

---

## 🚀 Why EnterSandBox?

The rise of autonomous AI agents has created new requirements for infrastructure.

*   **Untrusted Code Execution:** Agents generate unknown and potentially dangerous code.
*   **Latency Sensitivity:** Millisecond-level startup speeds are required to clear chat UX benchmarks.
*   **The Data Science Wall:** Lightweight WASM alone cannot run essential libraries like Pandas and NumPy.
*   **Lack of Governance:** Traditional approaches cannot prevent Data Loss Prevention (DLP) issues or unintended access by agents.

EnterSandBox is the **"OS for AI Agents"** that solves these challenges.

## ✨ Key Features

### 1. Hybrid Runtime Architecture
Dynamically selects and routes to the optimal runtime based on task characteristics.

| Tier | Name | Tech Stack | Startup Speed | Use Case |
| --- | --- | --- | --- | --- |
| **Tier 1** | **Nano-Sandbox** | Wasmtime + RustPython | **< 10ms** | Control logic, string manipulation, JSON parsing |
| **Tier 2** | **Heavy-Sandbox** | Firecracker MicroVM | **~150ms** | Data Analysis (Pandas), Machine Learning, Complex Dependencies |

### 2. Agency Governance (Network DLP)
Prevents rogue agent behavior and ensures enterprise compliance.

*   **PII Scanning:** Real-time inspection of communication content to block leaks of API keys or personal information.
*   **Intent-based Whitelist:** Dynamically restricts accessible domains based on the agent's "current intent".
*   **Audit Logs:** Records all actions and communications, providing complete traceability.

### 3. Time Travel Debugging
Revolutionizes the developer experience (DX) with advanced debugging capabilities.

*   **Stepwise Snapshots:** Saves memory and disk state at each execution step.
*   **Rewind & Inspect:** "Rewind" to the state immediately before an error occurred to inspect variable values and file contents.

## 🛠 Architecture

```mermaid
graph TD
    UserCode[User Code / Agent Action] --> Router[Adaptive Runtime Router]
    
    Router -->|Logic / Text Processing| Tier1["Tier 1: Nano-Sandbox (Wasm)"]
    Router -->|Data Science / Heavy Compute| Tier2["Tier 2: Heavy-Sandbox (MicroVM)"]
    
    subgraph Governance
        Sidecar[Network DLP Sidecar]
    end
    
    Tier1 -.-> Sidecar
    Tier2 -.-> Sidecar
    Sidecar --> Internet((Internet))
```

## 🧩 Usage (Preview)

Users can utilize a unified API without being conscious of the underlying runtime.

```python
from agentbox import Sandbox, SandboxConfig

config = SandboxConfig(memory_limit_mb=256, timeout_ms=3000, max_output_bytes=8 * 1024 * 1024)
box = Sandbox(config)

result = box.run("print('Hello from sandbox')")
print(result.stdout)
```

## 🧾 SDK API (Current)

- `Sandbox(config: Optional[SandboxConfig] = None)`
- `Sandbox.run(code: str) -> SandboxResult`
- `Sandbox.config -> SandboxConfig`
- `SandboxConfig(memory_limit_mb: Optional[int], timeout_ms: Optional[int], max_output_bytes: Optional[int])`
- `SandboxResult(stdout: str, stderr: str, exit_code: int)`

## 🧪 CPython WASI Repro Workflow (P1-070 to P1-077)

This repository includes a debug-only CPython WASI repro harness used to investigate and prevent
WASI startup regressions.

### Scope and constraints

- Repro helpers are private debug APIs on `agentbox._core`:
  - `_debug_run_cpython_wasi_repro(profile, code=None, timeout_ms=None, max_output_bytes=None)`
  - `_debug_describe_cpython_wasi_context_diff()`
- `Sandbox.run()` does **not** execute this CPython WASI path yet (`P1-078` is still open).
- `sdk-legacy` intentionally keeps the old preopen path mapping (`/sandbox`) so regression tests
  can continue to verify the historical failure (`No module named 'encodings'`).

### 1) Prepare pinned runtime assets

```bash
python3 scripts/prepare_cpython_wasi_assets.py
python3 scripts/prepare_cpython_wasi_assets.py --check-only
```

- Manifest: `assets/cpython-wasi/manifest.json`
- Pinned artifact (current): CPython WASI `3.13.12` (`python-3.13.12-wasi_sdk-24.zip`)
- For details, see `assets/cpython-wasi/README.md`

### 2) Inspect CLI/SDK context diff

```python
from agentbox import _core

print(_core._debug_describe_cpython_wasi_context_diff())
```

Key expectations fixed by tests:
- `preopen.guest_path` is aligned (`/`) between `cli` and `sdk`
- clocks/RNG sources are `same-source`
- `random.insecure_seed` is `runtime-generated` by design

### 3) Reproduce success/failure paths

```python
from agentbox import _core

success, stdout, stderr, error = _core._debug_run_cpython_wasi_repro(
    "sdk",
    "import json\nprint('ok')\n",
    timeout_ms=50,
    max_output_bytes=4096,
)
```

- Supported profiles: `cli`, `sdk`, `sdk-legacy`
- Failure traces include `trace.capture=wasm-backtrace-v1`
- Timeout and output-cap checks are regression-tested in both Rust and Python

### 4) Run regression suites

```bash
cd agentbox-core && cargo test cpython_wasi_repro -- --nocapture
pytest -q tests/python/test_cpython_wasi_repro.py
```

## 🗺 Roadmap

See [docs/PLAN.md](docs/PLAN.md) for details.

- **Phase 1:** Nano-Sandbox (MVP) - Ultra-fast execution environment based on Wasm
- **Phase 2:** Heavy-Sandbox & Routing - Firecracker integration and data science support
- **Phase 3:** Governance & Security - Network DLP and native MCP support
- **Phase 4:** Time Travel - Implementation of debugging functions and UI

## 📚 Documentation

- [Functional Specification (SPEC.md)](docs/SPEC.md) (Japanese)
- [Implementation Plan (PLAN.md)](docs/PLAN.md) (Japanese)
- [Research Report (RESEARCH.md)](docs/RESEARCH.md) (Japanese)
- [CPython WASI repro assets](assets/cpython-wasi/README.md)

## 🤝 Contributing

EnterSandBox is planned to be developed as an open-source project.
Contribution guidelines are being prepared.

## 📄 License

MIT License (Planned)
