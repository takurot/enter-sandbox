from typing import Optional

class SandboxConfig:
    memory_limit_mb: Optional[int]
    timeout_ms: Optional[int]
    def __init__(
        self, memory_limit_mb: Optional[int] = None, timeout_ms: Optional[int] = None
    ) -> None: ...

class Sandbox:
    def __init__(self, config: Optional[SandboxConfig] = None) -> None: ...
    def run(self, code: str) -> str: ...
    @property
    def config(self) -> SandboxConfig: ...
