import pytest

from agentbox import Sandbox, SandboxResult, SandboxConfig


def test_sandbox_run_basic():
    box = Sandbox()
    code = "print('Hello')"
    result = box.run(code)

    assert isinstance(result, SandboxResult)
    assert result.stdout.strip() == "Hello"
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
    # Actually print more than the limit
    code = "print('x' * 5000)"

    with pytest.raises(RuntimeError, match="max_output_bytes"):
        box.run(code)


def test_sandbox_timeout_error_message():
    box = Sandbox(SandboxConfig(timeout_ms=20))
    # Real Python infinite loop or long loop
    code = "while True: pass"

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
    box = Sandbox(SandboxConfig(allowed_modules=[]))
    # Even with an empty allow-list, relative imports must not raise a static analysis block.
    # At runtime, this will fail because it's not a package, so we catch it.
    result = box.run(
        "try:\n    from . import utils\nexcept (ImportError, ValueError):\n    pass\nprint('ok')"
    )
    assert result.exit_code == 0
    assert result.stdout.strip() == "ok"


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

    # Step 1: Write a file via Python
    box.run("with open('test_file.txt', 'w') as f: f.write('preserved_content')")

    # Step 2: In a separate run, verify that the file exists by including its content in output
    result = box.run("with open('test_file.txt', 'r') as f: print(f.read())")

    assert result.exit_code == 0
    assert result.stdout.strip() == "preserved_content"


def test_sandbox_run_empty_code():
    box = Sandbox()
    result = box.run("")
    assert result.exit_code == 0
    assert result.stdout == ""


def test_sandbox_utf8_handling():
    box = Sandbox()
    # UTF-8 in code and expected in stdout
    code = "print('こんにちは, 🌍')"
    result = box.run(code)
    assert result.exit_code == 0
    assert result.stdout.strip() == "こんにちは, 🌍"


def test_sandbox_run_multiple_consecutive():
    box = Sandbox()
    for i in range(5):
        result = box.run(f"print('run {i}')")
        assert result.exit_code == 0
        assert f"run {i}" in result.stdout


def test_sandbox_vfs_directive_rejects_parent_traversal():
    """Verify that file writes outside the sandbox are not allowed."""
    box = Sandbox()
    # In CPython WASI, '..' is handled by WASI if preopened.
    # Our preopen at /sandbox prevents escapes.
    code = "import os\ntry:\n    with open('../escape.txt', 'w') as f: f.write('hack')\nexcept Exception as e:\n    print(e)"
    result = box.run(code)

    # In WASI, this should fail at the system call level (likely FileNotFoundError, PermissionError, or Operation not permitted)
    assert result.exit_code == 0  # Exception caught
    assert any(
        msg in result.stdout
        for msg in [
            "No such file or directory",
            "not found",
            "Permission denied",
            "Operation not permitted",
        ]
    )


def test_sandbox_rejects_absolute_path_write_outside_sandbox():
    box = Sandbox()
    code = (
        "try:\n"
        "    with open('/escape_abs.txt', 'w') as f:\n"
        "        f.write('hack')\n"
        "    print('WRITE_SUCCEEDED')\n"
        "except Exception as e:\n"
        "    print(type(e).__name__)"
    )
    result = box.run(code)

    assert result.exit_code == 0
    assert "WRITE_SUCCEEDED" not in result.stdout
    assert any(
        name in result.stdout for name in ["PermissionError", "OSError", "FileNotFoundError"]
    )
