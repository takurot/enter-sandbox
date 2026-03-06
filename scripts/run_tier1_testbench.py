#!/usr/bin/env python3
import argparse
import json
import math
import platform
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from time import perf_counter
from typing import Any, Callable, Dict, List, Optional, Sequence


REPORT_VERSION = "tb-v1"
REPO_ROOT = Path(__file__).resolve().parents[1]
BENCHMARK_GUARD_SCRIPT = REPO_ROOT / "scripts" / "check_tier1_benchmarks.py"
QUICK_SCENARIOS = ["TB-01", "TB-03", "TB-04"]
FULL_SCENARIOS = ["TB-01", "TB-02", "TB-03", "TB-04", "TB-05", "TB-06"]
PERF_SCENARIOS = ["TB-01"]


class ScenarioFailure(AssertionError):
    def __init__(
        self,
        *,
        step: int,
        expected: str,
        actual: str,
        message: str,
    ) -> None:
        super().__init__(message)
        self.step = step
        self.expected = expected
        self.actual = actual
        self.message = message


@dataclass(frozen=True)
class ScenarioDefinition:
    id: str
    name: str
    executor: Callable[[], None]


def _import_agentbox() -> Any:
    try:
        from agentbox import Sandbox, SandboxConfig
    except Exception as error:  # pragma: no cover - environment dependent
        raise RuntimeError(
            "Failed to import agentbox. Build/install the package first "
            "(e.g. `maturin develop` or `pip install .`)."
        ) from error
    return Sandbox, SandboxConfig


def _ensure(condition: bool, *, step: int, expected: str, actual: str, message: str) -> None:
    if not condition:
        raise ScenarioFailure(
            step=step,
            expected=expected,
            actual=actual,
            message=message,
        )


def _contains(text: str, needle: str, *, step: int, label: str) -> None:
    _ensure(
        needle in text,
        step=step,
        expected=f"{label} contains {needle!r}",
        actual=text.strip(),
        message=f"{label} did not include expected text",
    )


def _trim_text(text: str, limit: int = 600) -> str:
    if len(text) <= limit:
        return text
    return text[: limit - 3] + "..."


def scenario_tb01_basic_run() -> None:
    Sandbox, _ = _import_agentbox()
    box = Sandbox()
    result = box.run("print('hello')")
    _ensure(
        result.exit_code == 0,
        step=1,
        expected="exit_code == 0",
        actual=f"exit_code == {result.exit_code}",
        message="unexpected exit code",
    )
    _ensure(
        result.stdout.strip() == "hello",
        step=1,
        expected="stdout.strip() == 'hello'",
        actual=result.stdout.strip(),
        message="stdout mismatch",
    )
    _ensure(
        result.stderr == "",
        step=1,
        expected="stderr == ''",
        actual=result.stderr,
        message="stderr should be empty",
    )


def scenario_tb02_multi_turn_vfs() -> None:
    Sandbox, _ = _import_agentbox()
    box = Sandbox()
    write_result = box.run("with open('tb_shared.txt', 'w') as f:\n    f.write('persisted-value')")
    _ensure(
        write_result.exit_code == 0,
        step=1,
        expected="write run exit_code == 0",
        actual=f"exit_code == {write_result.exit_code}",
        message="write step failed",
    )

    read_result = box.run("with open('tb_shared.txt', 'r') as f:\n    print(f.read())")
    _ensure(
        read_result.exit_code == 0,
        step=2,
        expected="read run exit_code == 0",
        actual=f"exit_code == {read_result.exit_code}",
        message="read step failed",
    )
    _ensure(
        read_result.stdout.strip() == "persisted-value",
        step=2,
        expected="stdout.strip() == 'persisted-value'",
        actual=read_result.stdout.strip(),
        message="VFS persistence mismatch",
    )


def scenario_tb03_import_guardrail() -> None:
    Sandbox, SandboxConfig = _import_agentbox()

    allow_box = Sandbox(SandboxConfig(allowed_modules=["json", "collections"]))
    allow_result = allow_box.run(
        "import json\nfrom collections import defaultdict\n"
        "print(json.dumps({'ok': bool(defaultdict(int))}))"
    )
    _ensure(
        allow_result.exit_code == 0,
        step=1,
        expected="allow-list import succeeds",
        actual=f"exit_code == {allow_result.exit_code}",
        message="allowed imports should succeed",
    )

    block_box = Sandbox(SandboxConfig(allowed_modules=["json", "collections"]))
    try:
        block_box.run("import json\nimport os")
    except RuntimeError as error:
        _contains(
            str(error),
            "Import blocked by SandboxConfig.allowed_modules",
            step=2,
            label="error",
        )
    else:  # pragma: no cover - guarded by assertion
        raise ScenarioFailure(
            step=2,
            expected="RuntimeError for disallowed import",
            actual="run succeeded",
            message="disallowed import should fail",
        )


def scenario_tb04_resource_limits() -> None:
    Sandbox, SandboxConfig = _import_agentbox()

    timeout_box = Sandbox(SandboxConfig(timeout_ms=20))
    try:
        timeout_box.run("while True:\n    pass")
    except RuntimeError as error:
        _contains(
            str(error),
            "Execution timed out after 20 ms",
            step=1,
            label="timeout error",
        )
    else:  # pragma: no cover - guarded by assertion
        raise ScenarioFailure(
            step=1,
            expected="RuntimeError for timeout",
            actual="run succeeded",
            message="timeout scenario should fail",
        )

    output_box = Sandbox(SandboxConfig(max_output_bytes=256))
    try:
        output_box.run("print('x' * 5000)")
    except RuntimeError as error:
        _contains(
            str(error),
            "max_output_bytes=256",
            step=2,
            label="output-limit error",
        )
    else:  # pragma: no cover - guarded by assertion
        raise ScenarioFailure(
            step=2,
            expected="RuntimeError for max_output_bytes",
            actual="run succeeded",
            message="output-limit scenario should fail",
        )


def scenario_tb05_error_semantics() -> None:
    Sandbox, _ = _import_agentbox()
    box = Sandbox()

    try:
        error_result = box.run("raise RuntimeError('tb05-boom')")
    except RuntimeError as error:
        _contains(
            str(error),
            "tb05-boom",
            step=1,
            label="raised RuntimeError",
        )
    else:
        _ensure(
            error_result.exit_code != 0,
            step=1,
            expected="non-zero exit_code for unhandled exception",
            actual=f"exit_code == {error_result.exit_code}",
            message="exception run should not return exit_code 0",
        )
        _contains(
            error_result.stderr,
            "tb05-boom",
            step=1,
            label="stderr",
        )

    exit_result = box.run("import sys\nsys.exit(123)")
    _ensure(
        exit_result.exit_code == 123,
        step=2,
        expected="exit_code == 123",
        actual=f"exit_code == {exit_result.exit_code}",
        message="sys.exit code mismatch",
    )


def scenario_tb06_lightweight_workflow() -> None:
    Sandbox, _ = _import_agentbox()
    box = Sandbox()
    workflow_code = (
        "import datetime\n"
        "import json\n"
        "import re\n"
        "payload = json.loads('{\"text\":\"id=42\",\"day\":\"2026-03-06\"}')\n"
        "match = re.search(r'(\\d+)', payload['text'])\n"
        "day = datetime.date.fromisoformat(payload['day'])\n"
        "result = {'id': int(match.group(1)), 'date': day.isoformat(), 'weekday': day.weekday()}\n"
        "print(json.dumps(result, sort_keys=True))\n"
    )
    result = box.run(workflow_code)
    _ensure(
        result.exit_code == 0,
        step=1,
        expected="exit_code == 0",
        actual=f"exit_code == {result.exit_code}",
        message="workflow run failed",
    )
    _ensure(
        result.stdout.strip() == '{"date": "2026-03-06", "id": 42, "weekday": 4}',
        step=1,
        expected='stdout == {"date": "2026-03-06", "id": 42, "weekday": 4}',
        actual=result.stdout.strip(),
        message="workflow output mismatch",
    )


SCENARIOS: Dict[str, ScenarioDefinition] = {
    "TB-01": ScenarioDefinition("TB-01", "Basic Run", scenario_tb01_basic_run),
    "TB-02": ScenarioDefinition("TB-02", "Multi-turn VFS", scenario_tb02_multi_turn_vfs),
    "TB-03": ScenarioDefinition("TB-03", "Import Guardrail", scenario_tb03_import_guardrail),
    "TB-04": ScenarioDefinition("TB-04", "Resource Limits", scenario_tb04_resource_limits),
    "TB-05": ScenarioDefinition("TB-05", "Error Semantics", scenario_tb05_error_semantics),
    "TB-06": ScenarioDefinition("TB-06", "Lightweight Workflow", scenario_tb06_lightweight_workflow),
}


def _positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("iterations must be >= 1")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run Tier 1 user-journey test bench scenarios for EnterSandBox."
    )
    parser.add_argument(
        "--mode",
        choices=["quick", "full", "perf"],
        default="quick",
        help="Execution mode: quick (PR), full (nightly), perf (quick + benchmark guard).",
    )
    parser.add_argument(
        "--iterations",
        type=_positive_int,
        help="Override scenario iteration count for the selected mode.",
    )
    parser.add_argument(
        "--json-output",
        type=Path,
        help="Optional path to write machine-readable JSON report.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print planned scenarios and output report without executing sandbox runs.",
    )
    parser.add_argument(
        "--perf-cold-start-output-file",
        type=Path,
        help="Forwarded to benchmark guard for deterministic perf-mode tests.",
    )
    parser.add_argument(
        "--perf-memory-output-file",
        type=Path,
        help="Forwarded to benchmark guard for deterministic perf-mode tests.",
    )
    parser.add_argument(
        "--perf-cold-start-threshold-ms",
        type=float,
        help="Override cold-start threshold (ms) passed to benchmark guard.",
    )
    parser.add_argument(
        "--perf-warm-peak-threshold-kb",
        type=int,
        help="Override warm peak threshold (KB) passed to benchmark guard.",
    )
    parser.add_argument(
        "--perf-command-timeout-sec",
        type=float,
        help="Override benchmark command timeout (seconds) passed to benchmark guard.",
    )
    return parser.parse_args()


def scenario_ids_for_mode(mode: str) -> List[str]:
    if mode == "quick":
        return QUICK_SCENARIOS
    if mode == "full":
        return FULL_SCENARIOS
    return PERF_SCENARIOS


def default_iterations_for_mode(mode: str) -> int:
    if mode == "full":
        return 3
    return 1


def _percentile(values: Sequence[float], p: float) -> float:
    if not values:
        return 0.0
    sorted_values = sorted(values)
    if len(sorted_values) == 1:
        return sorted_values[0]
    rank = p * (len(sorted_values) - 1)
    low = math.floor(rank)
    high = math.ceil(rank)
    if low == high:
        return sorted_values[low]
    frac = rank - low
    return sorted_values[low] * (1.0 - frac) + sorted_values[high] * frac


def _duration_summary(durations: Sequence[float]) -> Dict[str, float]:
    return {
        "min": round(min(durations), 3) if durations else 0.0,
        "max": round(max(durations), 3) if durations else 0.0,
        "p50": round(_percentile(durations, 0.5), 3),
        "p95": round(_percentile(durations, 0.95), 3),
    }


def _error_excerpt(error: BaseException) -> str:
    return _trim_text(f"{type(error).__name__}: {error}")


def run_scenarios(
    *,
    scenario_ids: Sequence[str],
    iterations: int,
    dry_run: bool,
) -> List[Dict[str, Any]]:
    results: List[Dict[str, Any]] = []
    for scenario_id in scenario_ids:
        definition = SCENARIOS[scenario_id]
        if dry_run:
            results.append(
                {
                    "id": definition.id,
                    "name": definition.name,
                    "status": "skipped",
                    "iterations": iterations,
                    "passed_iterations": 0,
                    "failed_iterations": 0,
                    "success_rate": None,
                    "duration_ms": _duration_summary([]),
                    "failure_signature": None,
                    "failures": [],
                }
            )
            continue

        durations: List[float] = []
        failures: List[Dict[str, Any]] = []
        passed_iterations = 0

        for index in range(1, iterations + 1):
            started = perf_counter()
            try:
                definition.executor()
                passed_iterations += 1
            except Exception as error:
                failure_detail: Dict[str, Any] = {
                    "iteration": index,
                    "error_type": type(error).__name__,
                    "error_excerpt": _error_excerpt(error),
                }
                if isinstance(error, ScenarioFailure):
                    failure_detail.update(
                        {
                            "step": error.step,
                            "expected": error.expected,
                            "actual": _trim_text(error.actual, limit=320),
                            "message": error.message,
                        }
                    )
                failures.append(failure_detail)
            finally:
                durations.append((perf_counter() - started) * 1000.0)

        failed_iterations = iterations - passed_iterations
        first_failure = failures[0] if failures else None
        if first_failure:
            failure_signature = (
                f"{first_failure['error_type']}:{first_failure.get('step', 'na')}:"
                f"{first_failure.get('message', first_failure['error_excerpt'])}"
            )
        else:
            failure_signature = None

        results.append(
            {
                "id": definition.id,
                "name": definition.name,
                "status": "pass" if failed_iterations == 0 else "fail",
                "iterations": iterations,
                "passed_iterations": passed_iterations,
                "failed_iterations": failed_iterations,
                "success_rate": round(passed_iterations / iterations, 4),
                "duration_ms": _duration_summary(durations),
                "failure_signature": failure_signature,
                "failures": failures,
            }
        )
    return results


def run_benchmark_guard(args: argparse.Namespace) -> Dict[str, Any]:
    command: List[str] = [sys.executable, str(BENCHMARK_GUARD_SCRIPT)]
    if args.perf_cold_start_output_file is not None:
        command.extend(
            [
                "--cold-start-output-file",
                str(args.perf_cold_start_output_file),
            ]
        )
    if args.perf_memory_output_file is not None:
        command.extend(
            [
                "--memory-output-file",
                str(args.perf_memory_output_file),
            ]
        )
    if args.perf_cold_start_threshold_ms is not None:
        command.extend(
            [
                "--cold-start-threshold-ms",
                str(args.perf_cold_start_threshold_ms),
            ]
        )
    if args.perf_warm_peak_threshold_kb is not None:
        command.extend(
            [
                "--warm-peak-threshold-kb",
                str(args.perf_warm_peak_threshold_kb),
            ]
        )
    if args.perf_command_timeout_sec is not None:
        command.extend(
            [
                "--command-timeout-sec",
                str(args.perf_command_timeout_sec),
            ]
        )

    if args.dry_run:
        return {
            "status": "skipped",
            "command": command,
            "returncode": 0,
            "stdout_excerpt": "",
            "stderr_excerpt": "",
        }

    run = subprocess.run(command, capture_output=True, text=True)
    return {
        "status": "pass" if run.returncode == 0 else "fail",
        "command": command,
        "returncode": run.returncode,
        "stdout_excerpt": _trim_text(run.stdout.strip()),
        "stderr_excerpt": _trim_text(run.stderr.strip()),
    }


def _summary_counts(
    scenario_results: Sequence[Dict[str, Any]],
    benchmark_guard: Optional[Dict[str, Any]],
) -> Dict[str, int]:
    total = len(scenario_results)
    passed = sum(1 for item in scenario_results if item["status"] == "pass")
    failed = sum(1 for item in scenario_results if item["status"] == "fail")

    if benchmark_guard is not None:
        total += 1
        if benchmark_guard["status"] == "pass":
            passed += 1
        elif benchmark_guard["status"] == "fail":
            failed += 1

    return {
        "total": total,
        "passed": passed,
        "failed": failed,
    }


def build_report(
    *,
    mode: str,
    iterations: int,
    scenario_results: Sequence[Dict[str, Any]],
    benchmark_guard: Optional[Dict[str, Any]],
) -> Dict[str, Any]:
    return {
        "version": REPORT_VERSION,
        "mode": mode,
        "iterations_per_scenario": iterations,
        "timestamp_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "environment": {
            "python": platform.python_version(),
            "platform": platform.platform(),
            "machine": platform.machine(),
        },
        "scenarios": list(scenario_results),
        "benchmark_guard": benchmark_guard,
        "summary": _summary_counts(scenario_results, benchmark_guard),
    }


def write_report(path: Path, report: Dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def print_report_summary(report: Dict[str, Any]) -> None:
    print(
        "[testbench] mode={mode} total={total} passed={passed} failed={failed}".format(
            mode=report["mode"],
            total=report["summary"]["total"],
            passed=report["summary"]["passed"],
            failed=report["summary"]["failed"],
        )
    )
    for item in report["scenarios"]:
        print(
            "[scenario] {id} {status} p50={p50:.3f}ms p95={p95:.3f}ms".format(
                id=item["id"],
                status=item["status"],
                p50=item["duration_ms"]["p50"],
                p95=item["duration_ms"]["p95"],
            )
        )
    if report["benchmark_guard"] is not None:
        guard = report["benchmark_guard"]
        print(
            "[benchmark-guard] {status} returncode={code}".format(
                status=guard["status"], code=guard["returncode"]
            )
        )


def main() -> int:
    args = parse_args()
    scenario_ids = scenario_ids_for_mode(args.mode)
    iterations = args.iterations or default_iterations_for_mode(args.mode)

    scenario_results = run_scenarios(
        scenario_ids=scenario_ids,
        iterations=iterations,
        dry_run=args.dry_run,
    )
    benchmark_guard: Optional[Dict[str, Any]] = None
    if args.mode == "perf":
        benchmark_guard = run_benchmark_guard(args)

    report = build_report(
        mode=args.mode,
        iterations=iterations,
        scenario_results=scenario_results,
        benchmark_guard=benchmark_guard,
    )
    print_report_summary(report)

    if args.json_output is not None:
        write_report(args.json_output, report)
        print(f"[testbench] wrote json report: {args.json_output}")

    if args.dry_run:
        return 0
    return 1 if report["summary"]["failed"] > 0 else 0


if __name__ == "__main__":
    raise SystemExit(main())
