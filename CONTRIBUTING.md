# Contributing to EnterSandBox

Thank you for your interest in EnterSandBox!

## Development Setup

### Prerequisites

-   **Rust**: [rustup](https://rustup.rs/) (Stable)
-   **Python**: 3.8+
-   **maturin**: `pip install maturin`

### Environment Initialization

```bash
# Initialize Python virtual environment
python3 -m venv .venv
source .venv/bin/activate

# Install dev dependencies
pip install -r requirements-dev.txt # Or manual: pip install maturin ruff pytest
```

### Building the Project

We use `maturin` to build the Rust core and link it to the Python SDK.

```bash
# Inside the virtual environment
maturin develop
```

### Running Tests

#### Rust Tests

```bash
cd agentbox-core
cargo test
```

#### Python Tests

```bash
pytest tests/python
```

### Code Quality

-   **Rust**: `cargo clippy` and `cargo fmt`
-   **Python**: `ruff check` and `ruff format`

## Project Structure

-   `agentbox-core/`: Core Rust implementation (Wasmtime, WASI, VFS).
-   `python/agentbox/`: Python SDK source.
-   `docs/`: Specification and design documents.
