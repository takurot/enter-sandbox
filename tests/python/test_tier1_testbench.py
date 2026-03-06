import json
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "scripts" / "run_tier1_testbench.py"


def _write_benchmark_outputs(
    tmp_path: Path,
    *,
    cold_median_ms: float = 19.755,
    warm_peak_kb: int = 30_720,
):
    cold_output = tmp_path / "cold_start_output.txt"
    cold_output.write_text(
        (
            "Benchmarking tier1_cold_start/sandbox_run_print_hello\n"
            "time:   [18.690 ms {median:.3f} ms 20.585 ms]\n"
        ).format(median=cold_median_ms),
        encoding="utf-8",
    )

    memory_output = tmp_path / "memory_output.txt"
    memory_output.write_text(
        (
            "--- Tier 1 Memory Usage Benchmark ---\n"
            "[Scenario B: Warm Start (Reuse Runtime)]\n"
            "Warm Run 10: Peak RSS: {peak} KB (process peak)\n"
            "Final Peak for Warm Scenario: {peak} KB\n"
        ).format(peak=warm_peak_kb),
        encoding="utf-8",
    )

    return cold_output, memory_output


def test_tier1_testbench_dry_run_quick_writes_json(tmp_path: Path):
    json_output = tmp_path / "tb_quick_dry.json"
    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--mode",
            "quick",
            "--dry-run",
            "--json-output",
            str(json_output),
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 0, run.stdout + run.stderr
    payload = json.loads(json_output.read_text(encoding="utf-8"))
    assert payload["mode"] == "quick"
    assert [item["id"] for item in payload["scenarios"]] == ["TB-01", "TB-03", "TB-04"]
    assert all(item["status"] == "skipped" for item in payload["scenarios"])


def test_tier1_testbench_quick_executes_successfully(tmp_path: Path):
    json_output = tmp_path / "tb_quick.json"
    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--mode",
            "quick",
            "--json-output",
            str(json_output),
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 0, run.stdout + run.stderr
    payload = json.loads(json_output.read_text(encoding="utf-8"))
    assert payload["mode"] == "quick"
    assert payload["summary"]["failed"] == 0
    assert all(item["status"] == "pass" for item in payload["scenarios"])


def test_tier1_testbench_perf_mode_passes_with_fixture_outputs(tmp_path: Path):
    cold_output, memory_output = _write_benchmark_outputs(tmp_path)
    json_output = tmp_path / "tb_perf_pass.json"
    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--mode",
            "perf",
            "--perf-cold-start-output-file",
            str(cold_output),
            "--perf-memory-output-file",
            str(memory_output),
            "--perf-cold-start-threshold-ms",
            "30",
            "--perf-warm-peak-threshold-kb",
            "80000",
            "--json-output",
            str(json_output),
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 0, run.stdout + run.stderr
    payload = json.loads(json_output.read_text(encoding="utf-8"))
    assert payload["mode"] == "perf"
    assert payload["summary"]["failed"] == 0
    assert payload["benchmark_guard"]["status"] == "pass"


def test_tier1_testbench_perf_mode_fails_when_guard_fails(tmp_path: Path):
    cold_output, memory_output = _write_benchmark_outputs(
        tmp_path,
        cold_median_ms=40.0,
        warm_peak_kb=90_000,
    )
    json_output = tmp_path / "tb_perf_fail.json"
    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--mode",
            "perf",
            "--perf-cold-start-output-file",
            str(cold_output),
            "--perf-memory-output-file",
            str(memory_output),
            "--perf-cold-start-threshold-ms",
            "30",
            "--perf-warm-peak-threshold-kb",
            "80000",
            "--json-output",
            str(json_output),
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 1
    payload = json.loads(json_output.read_text(encoding="utf-8"))
    assert payload["mode"] == "perf"
    assert payload["summary"]["failed"] == 1
    assert payload["benchmark_guard"]["status"] == "fail"
