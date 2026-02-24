#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys
from pathlib import Path
from typing import List, Optional, Sequence

DEFAULT_COLD_START_COMMAND = [
    "cargo",
    "bench",
    "--manifest-path",
    "agentbox-core/Cargo.toml",
    "--bench",
    "cold_start",
    "--",
    "--noplot",
]

DEFAULT_MEMORY_COMMAND = [
    "cargo",
    "bench",
    "--manifest-path",
    "agentbox-core/Cargo.toml",
    "--bench",
    "memory_usage",
    "--",
    "warm",
]

DEFAULT_COLD_START_THRESHOLD_MS = 80.0
DEFAULT_WARM_PEAK_THRESHOLD_KB = 80 * 1024

CRITERION_TIME_PATTERN = re.compile(
    r"time:\s*\[\s*([0-9]+(?:\.[0-9]+)?)\s*([a-zA-Zµμ]+)\s+"
    r"([0-9]+(?:\.[0-9]+)?)\s*([a-zA-Zµμ]+)\s+"
    r"([0-9]+(?:\.[0-9]+)?)\s*([a-zA-Zµμ]+)\s*\]"
)
WARM_PEAK_PATTERN = re.compile(r"Final Peak for Warm Scenario:\s*([0-9]+)\s*KB")

UNIT_TO_MILLISECONDS = {
    "ns": 1e-6,
    "us": 1e-3,
    "µs": 1e-3,
    "μs": 1e-3,
    "ms": 1.0,
    "s": 1000.0,
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run Tier 1 benchmarks and fail when cold-start latency or warm memory "
            "usage regresses beyond configured thresholds."
        )
    )
    parser.add_argument(
        "--cold-start-threshold-ms",
        type=float,
        default=DEFAULT_COLD_START_THRESHOLD_MS,
        help="Maximum allowed cold-start benchmark median in milliseconds.",
    )
    parser.add_argument(
        "--warm-peak-threshold-kb",
        type=int,
        default=DEFAULT_WARM_PEAK_THRESHOLD_KB,
        help="Maximum allowed warm scenario peak RSS in KB.",
    )
    parser.add_argument(
        "--cold-start-output-file",
        type=Path,
        help="Read cold-start benchmark output from file instead of running cargo bench.",
    )
    parser.add_argument(
        "--memory-output-file",
        type=Path,
        help="Read memory benchmark output from file instead of running cargo bench.",
    )
    parser.add_argument(
        "--cold-start-command",
        nargs="+",
        default=DEFAULT_COLD_START_COMMAND,
        help="Command used for the cold-start benchmark.",
    )
    parser.add_argument(
        "--memory-command",
        nargs="+",
        default=DEFAULT_MEMORY_COMMAND,
        help="Command used for the memory benchmark.",
    )
    return parser.parse_args()


def unit_to_ms(value_text: str, unit_text: str) -> float:
    unit = unit_text.strip()
    if unit not in UNIT_TO_MILLISECONDS:
        raise ValueError("Unsupported time unit in criterion output: {unit}".format(unit=unit))
    return float(value_text) * UNIT_TO_MILLISECONDS[unit]


def parse_cold_start_median_ms(output: str) -> float:
    matches = list(CRITERION_TIME_PATTERN.finditer(output))
    if not matches:
        raise ValueError("Could not find criterion 'time: [.. .. ..]' output.")
    match = matches[-1]

    median_value = match.group(3)
    median_unit = match.group(4)
    return unit_to_ms(median_value, median_unit)


def parse_warm_peak_kb(output: str) -> int:
    matches = list(WARM_PEAK_PATTERN.finditer(output))
    if not matches:
        raise ValueError("Could not find warm scenario peak RSS output.")
    return int(matches[-1].group(1))


def load_output_from_file(path: Path) -> str:
    if not path.exists():
        raise FileNotFoundError("Output file not found: {path}".format(path=path))
    return path.read_text(encoding="utf-8")


def run_command(command: Sequence[str]) -> str:
    print("Running: {command}".format(command=" ".join(command)))
    run = subprocess.run(list(command), capture_output=True, text=True)
    output = "{stdout}\n{stderr}".format(stdout=run.stdout, stderr=run.stderr)
    if run.returncode != 0:
        raise RuntimeError(
            "Command failed ({code}): {command}\n{output}".format(
                code=run.returncode,
                command=" ".join(command),
                output=output,
            )
        )
    return output


def acquire_output(
    *,
    output_file: Optional[Path],
    command: Sequence[str],
) -> str:
    if output_file is not None:
        print("Reading benchmark output from: {path}".format(path=output_file))
        return load_output_from_file(output_file)
    return run_command(command)


def main() -> int:
    args = parse_args()

    try:
        cold_output = acquire_output(
            output_file=args.cold_start_output_file,
            command=args.cold_start_command,
        )
        memory_output = acquire_output(
            output_file=args.memory_output_file,
            command=args.memory_command,
        )

        cold_start_median_ms = parse_cold_start_median_ms(cold_output)
        warm_peak_kb = parse_warm_peak_kb(memory_output)
    except Exception as error:  # pragma: no cover - surfaced by integration tests
        print("Benchmark regression guard failed to collect metrics: {error}".format(error=error))
        return 2

    print("Cold start median: {value:.3f} ms".format(value=cold_start_median_ms))
    print("Warm peak RSS: {value} KB".format(value=warm_peak_kb))
    print(
        "Thresholds: cold_start<= {cold:.3f} ms, warm_peak<= {warm} KB".format(
            cold=args.cold_start_threshold_ms,
            warm=args.warm_peak_threshold_kb,
        )
    )

    violations: List[str] = []
    if cold_start_median_ms > args.cold_start_threshold_ms:
        violations.append(
            "Cold start median {actual:.3f} ms exceeds threshold {limit:.3f} ms".format(
                actual=cold_start_median_ms,
                limit=args.cold_start_threshold_ms,
            )
        )
    if warm_peak_kb > args.warm_peak_threshold_kb:
        violations.append(
            "Warm peak RSS {actual} KB exceeds threshold {limit} KB".format(
                actual=warm_peak_kb,
                limit=args.warm_peak_threshold_kb,
            )
        )

    if violations:
        print("Benchmark regression guard failed:", file=sys.stderr)
        for message in violations:
            print("- {message}".format(message=message), file=sys.stderr)
        return 1

    print("Benchmark regression check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
