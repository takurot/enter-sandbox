import pytest

from agentbox import Sandbox, SandboxResult


def test_sandbox_run_basic():
    box = Sandbox()
    code = "print('Hello')"
    result = box.run(code)

    assert isinstance(result, SandboxResult)
    assert "Start Execution" in result.stdout
    assert "Executing code: print('Hello')" in result.stdout
    assert "End Execution" in result.stdout
    assert result.stderr == ""
    assert result.exit_code == 0


def test_sandbox_config():
    from agentbox import SandboxConfig

    config = SandboxConfig(
        memory_limit_mb=100,
        timeout_ms=5000,
        max_output_bytes=4096,
        allowed_modules=["json", "collections"],
    )
    box = Sandbox(config)
    assert box.config.memory_limit_mb == 100
    assert box.config.timeout_ms == 5000
    assert box.config.max_output_bytes == 4096
    assert box.config.allowed_modules == ["json", "collections"]


def test_sandbox_output_limit_error_message():
    from agentbox import SandboxConfig

    box = Sandbox(SandboxConfig(max_output_bytes=1024))
    code = "x" * 5000

    with pytest.raises(RuntimeError, match="max_output_bytes"):
        box.run(code)


def test_sandbox_timeout_error_message():
    from agentbox import SandboxConfig

    box = Sandbox(SandboxConfig(timeout_ms=20))
    code = "__agentbox_spin_ms=250\nprint('slow')"

    with pytest.raises(RuntimeError, match="Execution timed out after 20 ms"):
        box.run(code)


def test_sandbox_allowed_modules_accepts_allowed_imports():
    from agentbox import SandboxConfig

    box = Sandbox(SandboxConfig(allowed_modules=["json", "collections"]))
    result = box.run("import json\nfrom collections import defaultdict\nprint('ok')")

    assert result.exit_code == 0
    assert "Executing code:" in result.stdout


def test_sandbox_allowed_modules_blocks_disallowed_imports():
    from agentbox import SandboxConfig

    box = Sandbox(SandboxConfig(allowed_modules=["json"]))
    code = "import json\nimport os\nfrom collections import defaultdict"

    with pytest.raises(
        RuntimeError,
        match=r"Import blocked by SandboxConfig.allowed_modules.*blocked=\[collections, os\].*allowed=\[json\]",
    ):
        box.run(code)


def test_sandbox_allowed_modules_blocks_tab_or_semicolon_import_forms():
    from agentbox import SandboxConfig

    box = Sandbox(SandboxConfig(allowed_modules=["json"]))
    code = "value = 1; import\tos"

    with pytest.raises(
        RuntimeError,
        match=r"Import blocked by SandboxConfig.allowed_modules.*blocked=\[os\]",
    ):
        box.run(code)
