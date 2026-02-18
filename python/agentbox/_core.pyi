from typing import Optional, Tuple

def _debug_run_cpython_wasi_repro(
    profile: str,
    code: Optional[str] = None,
    timeout_ms: Optional[int] = None,
    max_output_bytes: Optional[int] = None,
) -> Tuple[bool, str, str, Optional[str]]: ...
def _debug_describe_cpython_wasi_context_diff() -> str: ...

class SandboxConfig:
    memory_limit_mb: Optional[int]
    timeout_ms: Optional[int]
    max_output_bytes: Optional[int]
    def __init__(
        self,
        memory_limit_mb: Optional[int] = None,
        timeout_ms: Optional[int] = None,
        max_output_bytes: Optional[int] = None,
    ) -> None: ...

class SandboxResult:
    stdout: str
    stderr: str
    exit_code: int

class Sandbox:
    def __init__(self, config: Optional[SandboxConfig] = None) -> None: ...
    def run(self, code: str) -> SandboxResult: ...
    @property
    def config(self) -> SandboxConfig: ...
