from agentbox import Sandbox


def test_stdlib_json():
    sb = Sandbox()
    code = """
import json
data = {"a": 1, "b": [2, 3]}
print(json.dumps(data))
"""
    result = sb.run(code)
    assert result.exit_code == 0
    assert result.stdout.strip() == '{"a": 1, "b": [2, 3]}'


def test_stdlib_re():
    sb = Sandbox()
    code = """
import re
match = re.search(r"(\d+)", "abc123def")
if match:
    print(match.group(1))
"""
    result = sb.run(code)
    assert result.exit_code == 0
    assert result.stdout.strip() == "123"


def test_stdlib_datetime():
    sb = Sandbox()
    code = """
from datetime import datetime, date
d = date(2026, 2, 28)
print(d.isoformat())
"""
    result = sb.run(code)
    assert result.exit_code == 0
    assert result.stdout.strip() == "2026-02-28"


def test_stdlib_collections():
    sb = Sandbox()
    code = """
from collections import Counter, defaultdict
c = Counter("abracadabra")
print(c["a"])
d = defaultdict(int)
d["x"] += 1
print(d["x"])
"""
    result = sb.run(code)
    assert result.exit_code == 0
    assert "5" in result.stdout
    assert "1" in result.stdout


def test_stdlib_math_random():
    sb = Sandbox()
    code = """
import math
import random
print(math.isqrt(16))
# random should work without crashing
random.seed(42)
print(random.randint(1, 100))
"""
    result = sb.run(code)
    assert result.exit_code == 0
    assert "4" in result.stdout


def test_vfs_import_cross_file():
    sb = Sandbox()
    # We don't have a direct VFS write API in Sandbox yet,
    # but Sandbox uses a shared VFS internally.
    # However, each Sandbox.run() currently creates a fresh VFS?
    # Let's check Sandbox implementation in lib.rs.
    # Sandbox { runtime, vfs, config } -> vfs is persistent for the Sandbox instance!

    # Let's first verify if Sandbox.vfs is shared across runs.
    # In lib.rs:
    # fn run(&self, code: String) -> PyResult<SandboxResult> {
    #     execute_sandbox_run(&self.runtime, &self.vfs, &self.config, &code)
    # }

    # execute_sandbox_run calls sync_back_to_vfs at the end.

    code_write = """
with open("myself.py", "w") as f:
    f.write("def hello(): return 'world'")
"""
    sb.run(code_write)

    code_read = """
import myself
print(myself.hello())
"""
    result = sb.run(code_read)
    if result.exit_code != 0:
        print(f"FAILED stderr: {result.stderr}")
    assert result.exit_code == 0
    assert result.stdout.strip() == "world"


def test_sys_exit_code():
    sb = Sandbox()
    result = sb.run("import sys; sys.exit(123)")
    assert result.exit_code == 123


def test_pythonpath_includes_sandbox():
    sb = Sandbox()
    # Check if we can import a module from a subdirectory in VFS
    code_setup = """
import os
os.makedirs("subdir", exist_ok=True)
with open("subdir/mod.py", "w") as f:
    f.write("VAL = 42")
"""
    sb.run(code_setup)

    code_test = """
import sys
import subdir.mod
print(subdir.mod.VAL)
"""
    result = sb.run(code_test)
    if result.exit_code != 0:
        print(f"FAILED stderr: {result.stderr}")
    assert result.exit_code == 0
    assert result.stdout.strip() == "42"
