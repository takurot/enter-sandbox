import pytest

from agentbox import Sandbox


def test_sandbox_run_basic():
    box = Sandbox()
    code = "print('Hello')"
    result = box.run(code)

    assert "Start Execution" in result
    assert "Executing code: print('Hello')" in result
    assert "End Execution" in result


def test_sandbox_config():
    from agentbox import SandboxConfig

    config = SandboxConfig(memory_limit_mb=100, timeout_ms=5000, max_output_bytes=4096)
    box = Sandbox(config)
    assert box.config.memory_limit_mb == 100
    assert box.config.timeout_ms == 5000
    assert box.config.max_output_bytes == 4096


def test_sandbox_output_limit_error_message():
    from agentbox import SandboxConfig

    box = Sandbox(SandboxConfig(max_output_bytes=1024))
    code = "x" * 5000

    with pytest.raises(RuntimeError, match="max_output_bytes"):
        box.run(code)
