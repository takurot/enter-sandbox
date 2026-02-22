import pytest

from agentbox import Sandbox, SandboxResult, SandboxConfig


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
    box = Sandbox(SandboxConfig(max_output_bytes=1024))
    code = "x" * 5000

    with pytest.raises(RuntimeError, match="max_output_bytes"):
        box.run(code)


def test_sandbox_timeout_error_message():
    box = Sandbox(SandboxConfig(timeout_ms=20))
    code = "__agentbox_spin_ms=250\nprint('slow')"

    with pytest.raises(RuntimeError, match="Execution timed out after 20 ms"):
        box.run(code)


def test_sandbox_allowed_modules_accepts_allowed_imports():
    box = Sandbox(SandboxConfig(allowed_modules=["json", "collections"]))
    result = box.run("import json\nfrom collections import defaultdict\nprint('ok')")

    assert result.exit_code == 0


def test_sandbox_allowed_modules_blocks_disallowed_imports():
    box = Sandbox(SandboxConfig(allowed_modules=["json"]))
    code = "import json\nimport os\nfrom collections import defaultdict"

    with pytest.raises(RuntimeError) as exc_info:
        box.run(code)
    msg = str(exc_info.value)
    assert "Import blocked by SandboxConfig.allowed_modules" in msg
    assert "collections" in msg
    assert "os" in msg
    assert "allowed=[json]" in msg


def test_sandbox_allowed_modules_blocks_tab_or_semicolon_import_forms():
    box = Sandbox(SandboxConfig(allowed_modules=["json"]))
    code = "value = 1; import\tos"

    with pytest.raises(
        RuntimeError,
        match=r"Import blocked by SandboxConfig.allowed_modules.*blocked=\[os\]",
    ):
        box.run(code)


def test_sandbox_allowed_modules_ignores_import_text_inside_string_literals():
    box = Sandbox(SandboxConfig(allowed_modules=[]))
    result = box.run('print("safe; import os")')

    assert result.exit_code == 0


def test_sandbox_allowed_modules_blocks_parenthesised_from_import():
    """from x import (A, B) form must be detected and blocked."""
    box = Sandbox(SandboxConfig(allowed_modules=[]))
    code = "from collections import (\n    defaultdict,\n    OrderedDict,\n)"

    with pytest.raises(RuntimeError) as exc_info:
        box.run(code)
    assert "collections" in str(exc_info.value)


def test_sandbox_allowed_modules_allows_parenthesised_from_import_when_permitted():
    """from x import (A, B) form must pass when x is in allowed_modules."""
    box = Sandbox(SandboxConfig(allowed_modules=["collections"]))
    result = box.run(
        "from collections import (\n    defaultdict,\n    OrderedDict,\n)\nprint('ok')"
    )
    assert result.exit_code == 0


def test_sandbox_allowed_modules_ignores_relative_imports():
    """Relative imports (from . import x) are skipped, not blocked."""
    # Even with an empty allow-list, relative imports must not raise.
    box = Sandbox(SandboxConfig(allowed_modules=[]))
    # The Dummy runner won't actually execute Python, so we just check that
    # enforce_allowed_modules does not raise before reaching the WASM layer.
    result = box.run("from . import utils\nprint('ok')")
    assert result.exit_code == 0


def test_sandbox_config_defaults():
    config = SandboxConfig()
    assert config.memory_limit_mb is None

    box = Sandbox()
    assert box.config.memory_limit_mb == 512
    assert box.config.timeout_ms == 10000
    assert box.config.max_output_bytes == 8 * 1024 * 1024
    assert box.config.allowed_modules is None


@pytest.mark.parametrize(
    "code, allowed, expect_success, err_match",
    [
        ("import json, collections\nprint('ok')", ["json", "collections"], True, None),
        ("import json, os", ["json"], False, "Import blocked by SandboxConfig.allowed_modules"),
        ("import collections as col\nprint('ok')", ["collections"], True, None),
        ("import os as o", ["collections"], False, r"blocked=\[os\]"),
        ("from collections import defaultdict as dd\nprint('ok')", ["collections"], True, None),
        ("from os import path as p", ["collections"], False, r"blocked=\[os\]"),
        ("from collections import *\nprint('ok')", ["collections"], True, None),
        ("from os import *", ["collections"], False, r"blocked=\[os\]"),
    ],
)
def test_sandbox_allowed_modules_cases(code, allowed, expect_success, err_match):
    box = Sandbox(SandboxConfig(allowed_modules=allowed))
    if expect_success:
        assert box.run(code).exit_code == 0
    else:
        with pytest.raises(RuntimeError, match=err_match):
            box.run(code)


def test_sandbox_allowed_modules_dynamic_import_limitation():
    # dynamic imports are NOT blocked by the static analysis parser implementation.
    box = Sandbox(SandboxConfig(allowed_modules=["json"]))
    # It passes the static analysis blocker. The actual runtime behavior is tested separately.
    result = box.run("os = __import__('os')\nprint('ok')")
    assert result.exit_code == 0


def test_sandbox_vfs_persistence():
    """Verify that files written in one run are preserved for the next."""
    box = Sandbox()

    # Step 1: Write a file via the Dummy Runner directive
    box.run("__agentbox_write_file=test_dir/file.txt:preserved_content\nprint('step 1 ok')")

    # Step 2: In a separate run, verify that the file exists by including its content in output
    result = box.run("__agentbox_read_file=test_dir/file.txt\nprint('step 2 ok')")

    assert result.exit_code == 0
    assert "File test_dir/file.txt: preserved_content" in result.stdout


def test_sandbox_run_empty_code():
    box = Sandbox()
    result = box.run("")
    assert result.exit_code == 0
    assert "Executing code: " in result.stdout


def test_sandbox_utf8_handling():
    box = Sandbox()
    # UTF-8 in code and expected in stdout
    code = "print('こんにちは, 🌍')"
    result = box.run(code)
    assert result.exit_code == 0
    assert "Executing code: print('こんにちは, 🌍')" in result.stdout
    assert "Start Execution" in result.stdout


def test_sandbox_run_multiple_consecutive():
    box = Sandbox()
    for i in range(5):
        result = box.run(f"print('run {i}')")
        assert result.exit_code == 0
        assert f"run {i}" in result.stdout
