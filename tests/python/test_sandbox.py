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
    config = SandboxConfig(memory_limit_mb=100, timeout_ms=5000)
    box = Sandbox(config)
    assert box.config.memory_limit_mb == 100
    assert box.config.timeout_ms == 5000
