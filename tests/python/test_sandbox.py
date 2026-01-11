import pytest
from agentbox import Sandbox

def test_sandbox_run_basic(capfd):
    box = Sandbox()
    code = "print('Hello')"
    result = box.run(code)
    print(f"Result: {result}")
    
    # result is empty string with inherited stdio
    # assert "Output printed to host stdout" in result
    
    # Verify stdout was printed (since we use inherit_stdio)
    captured = capfd.readouterr()
    print(f"Captured stdout: {captured.out}")
    print(f"Captured stderr: {captured.err}")
    
    # The dummy runner usually prints:
    # Start Execution
    # Executing code: ...
    # End Execution
    
    assert "Start Execution" in captured.out
    assert "End Execution" in captured.out

def test_sandbox_config():
    from agentbox import SandboxConfig
    config = SandboxConfig(memory_limit_mb=100, timeout_ms=5000)
    box = Sandbox(config)
    assert box.config.memory_limit_mb == 100
    assert box.config.timeout_ms == 5000
