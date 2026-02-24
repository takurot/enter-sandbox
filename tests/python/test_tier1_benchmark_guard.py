import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "scripts" / "check_tier1_benchmarks.py"


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


def test_tier1_benchmark_guard_passes_when_within_threshold(tmp_path: Path):
    cold_output, memory_output = _write_benchmark_outputs(tmp_path)

    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--cold-start-output-file",
            str(cold_output),
            "--memory-output-file",
            str(memory_output),
            "--cold-start-threshold-ms",
            "30",
            "--warm-peak-threshold-kb",
            "40000",
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 0, run.stdout + run.stderr
    assert "Benchmark regression check passed." in run.stdout


def test_tier1_benchmark_guard_fails_when_threshold_is_exceeded(tmp_path: Path):
    cold_output, memory_output = _write_benchmark_outputs(
        tmp_path,
        cold_median_ms=40.0,
        warm_peak_kb=90_000,
    )

    run = subprocess.run(
        [
            sys.executable,
            str(SCRIPT_PATH),
            "--cold-start-output-file",
            str(cold_output),
            "--memory-output-file",
            str(memory_output),
            "--cold-start-threshold-ms",
            "30",
            "--warm-peak-threshold-kb",
            "80000",
        ],
        capture_output=True,
        text=True,
    )

    assert run.returncode == 1
    assert "Benchmark regression guard failed:" in run.stderr
    assert "Cold start median 40.000 ms exceeds threshold 30.000 ms" in run.stderr
    assert "Warm peak RSS 90000 KB exceeds threshold 80000 KB" in run.stderr
