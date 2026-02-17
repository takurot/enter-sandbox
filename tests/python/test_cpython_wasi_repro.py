from typing import Optional

from agentbox import _core

STDLIB_SMOKE_CODE = """import collections
import datetime
import json
import re

payload = {
    "json": json.loads('{"value": 42}')["value"],
    "regex": bool(re.search(r"\\d+", "v42")),
    "date": datetime.date(2026, 2, 15).isoformat(),
    "counter": collections.Counter("abca")["a"],
}

assert payload["json"] == 42
assert payload["regex"]
assert payload["date"] == "2026-02-15"
assert payload["counter"] == 2
print("stdlib-smoke:ok")
"""
EXCEPTION_CODE = "raise RuntimeError('boom-from-repro')\n"
TIMEOUT_TARGET_CODE = "import time\ntime.sleep(2)\nprint('timeout-missed')\n"
LARGE_OUTPUT_CODE = "print('x' * 16384)\n"


def _run_profile(
    profile: str,
    code: Optional[str] = None,
    timeout_ms: Optional[int] = None,
    max_output_bytes: Optional[int] = None,
):
    return _core._debug_run_cpython_wasi_repro(
        profile,
        code,
        timeout_ms,
        max_output_bytes,
    )


def _parse_context_diff_report(report: str):
    lines = [line.strip() for line in report.strip().splitlines() if line.strip()]
    assert lines

    header = [part.strip() for part in lines[0].split("|")]
    assert header == ["field", "cli", "sdk", "status", "note"]
    assert lines[1].startswith("---")

    rows = {}
    for line in lines[2:]:
        parts = [part.strip() for part in line.split("|")]
        assert len(parts) == 5
        field, cli, sdk, status, note = parts
        rows[field] = {
            "cli": cli,
            "sdk": sdk,
            "status": status,
            "note": note,
        }
    return rows


def _extract_frames_count(error_text: str) -> int:
    for line in error_text.splitlines():
        if line.startswith("wasm_backtrace.frames="):
            return int(line.split("=", maxsplit=1)[1])
    raise AssertionError("wasm_backtrace.frames line not found")


def test_cpython_wasi_cli_and_sdk_succeed():
    cli_success, cli_stdout, cli_stderr, cli_error = _run_profile("cli")
    sdk_success, sdk_stdout, sdk_stderr, sdk_error = _run_profile("sdk")

    assert cli_success, cli_error or cli_stderr
    assert "ok" in cli_stdout
    assert sdk_success, sdk_error or sdk_stderr
    assert "ok" in sdk_stdout


def test_cpython_wasi_cli_profile_imports_core_stdlib_modules():
    success, stdout, stderr, error = _run_profile("cli", STDLIB_SMOKE_CODE)

    assert success, error or stderr
    assert "stdlib-smoke:ok" in stdout


def test_cpython_wasi_sdk_profile_imports_core_stdlib_modules():
    success, stdout, stderr, error = _run_profile("sdk", STDLIB_SMOKE_CODE)

    assert success, error or stderr
    assert "stdlib-smoke:ok" in stdout


def test_cpython_wasi_sdk_legacy_failure_reports_missing_encodings():
    sdk_success, _, sdk_stderr, _ = _run_profile("sdk-legacy")

    assert not sdk_success
    assert "No module named 'encodings'" in sdk_stderr


def test_cpython_wasi_sdk_legacy_failure_includes_structured_trace_log():
    sdk_success, _, _, sdk_error = _run_profile("sdk-legacy")

    assert not sdk_success
    assert sdk_error is not None
    assert "trace.capture=wasm-backtrace-v1" in sdk_error
    assert "start.call.error" in sdk_error
    assert "trace.status=attached" in sdk_error
    assert "wasm_backtrace.note=not-attached" not in sdk_error
    assert _extract_frames_count(sdk_error) > 0


def test_cpython_wasi_sdk_profile_failure_reports_python_exception():
    success, _, stderr, _ = _run_profile("sdk", EXCEPTION_CODE)

    assert not success
    assert "RuntimeError: boom-from-repro" in stderr


def test_cpython_wasi_sdk_profile_timeout_is_reported():
    success, _, _, error = _run_profile("sdk", TIMEOUT_TARGET_CODE, timeout_ms=20)

    assert not success
    assert error is not None
    assert "Execution timed out after 20 ms" in error
    assert "trace.capture=wasm-backtrace-v1" in error


def test_cpython_wasi_sdk_profile_output_limit_is_reported():
    success, _, _, error = _run_profile("sdk", LARGE_OUTPUT_CODE, max_output_bytes=256)

    assert not success
    assert error is not None
    assert "max_output_bytes=256" in error


def test_cpython_wasi_context_diff_report_includes_required_dimensions():
    rows = _parse_context_diff_report(_core._debug_describe_cpython_wasi_context_diff())

    assert rows["argv"]["status"] == "same"
    assert rows["env"]["status"] == "same"
    assert rows["preopen.host_path"]["status"] == "same"
    assert rows["preopen.guest_path"]["status"] == "same"
    assert rows["stdio.stdin"]["status"] == "same"
    assert rows["stdio.stdout"]["status"] == "same"
    assert rows["stdio.stderr"]["status"] == "same"
    assert rows["clock.wall"]["status"] == "same-source"
    assert rows["clock.monotonic"]["status"] == "same-source"
    assert rows["random.secure"]["status"] == "same-source"
    assert rows["random.insecure"]["status"] == "same-source"
    assert rows["random.insecure_seed"]["status"] == "runtime-generated"


def test_cpython_wasi_context_diff_report_preopen_path_is_aligned():
    rows = _parse_context_diff_report(_core._debug_describe_cpython_wasi_context_diff())

    assert rows["preopen.guest_path"]["cli"] == "/"
    assert rows["preopen.guest_path"]["sdk"] == "/"
    assert rows["preopen.guest_path"]["status"] == "same"


def test_cpython_wasi_context_diff_report_masks_host_runtime_path():
    rows = _parse_context_diff_report(_core._debug_describe_cpython_wasi_context_diff())

    assert rows["preopen.host_path"]["cli"] == "<assets/cpython-wasi/runtime>"
    assert rows["preopen.host_path"]["sdk"] == "<assets/cpython-wasi/runtime>"
