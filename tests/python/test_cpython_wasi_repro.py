from agentbox import _core


def _run_profile(profile: str):
    return _core._debug_run_cpython_wasi_repro(profile)


def test_cpython_wasi_cli_success_but_sdk_failure():
    cli_success, cli_stdout, cli_stderr, cli_error = _run_profile("cli")
    sdk_success, _, _, _ = _run_profile("sdk")

    assert cli_success, cli_error or cli_stderr
    assert "ok" in cli_stdout
    assert not sdk_success


def test_cpython_wasi_sdk_failure_reports_missing_encodings():
    sdk_success, _, sdk_stderr, _ = _run_profile("sdk")

    assert not sdk_success
    assert "No module named 'encodings'" in sdk_stderr


def test_cpython_wasi_context_diff_report_includes_required_dimensions():
    report = _core._debug_describe_cpython_wasi_context_diff()

    assert "argv" in report
    assert "env" in report
    assert "preopen.guest_path" in report
    assert "stdio.stdin" in report
    assert "stdio.stdout" in report
    assert "stdio.stderr" in report
    assert "clock.wall" in report
    assert "clock.monotonic" in report
    assert "random.secure" in report
    assert "random.insecure" in report
    assert "random.insecure_seed" in report


def test_cpython_wasi_context_diff_report_detects_preopen_path_difference():
    report = _core._debug_describe_cpython_wasi_context_diff()

    assert "preopen.guest_path" in report
    assert " | / | /sandbox | different" in report
