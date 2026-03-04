import os
from pathlib import Path

from ._core import Sandbox, SandboxConfig, SandboxResult

_RUNTIME_ENV = "AGENTBOX_CPYTHON_WASI_RUNTIME_DIR"
_PACKAGE_DIR = Path(__file__).resolve().parent
_RUNTIME_CANDIDATES = (
    _PACKAGE_DIR / "_runtime" / "cpython-wasi" / "runtime",
    _PACKAGE_DIR.parent / "assets" / "cpython-wasi" / "runtime",
    _PACKAGE_DIR.parent.parent / "assets" / "cpython-wasi" / "runtime",
)

for _candidate in _RUNTIME_CANDIDATES:
    if _candidate.is_dir():
        os.environ.setdefault(_RUNTIME_ENV, str(_candidate))
        break

__all__ = ["Sandbox", "SandboxConfig", "SandboxResult"]
