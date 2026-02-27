#!/usr/bin/env python3
import argparse
import os
import shlex
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional, Sequence


DEFAULT_OUT_DIR = Path("dist")
DEFAULT_MANIFEST_PATH = Path("agentbox-core/Cargo.toml")
ABI3_FORWARD_COMPAT_ENV = "PYO3_USE_ABI3_FORWARD_COMPATIBILITY"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=("Build PyPI release artifacts for agentbox with maturin (wheel and sdist).")
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=DEFAULT_OUT_DIR,
        help="Output directory for built artifacts.",
    )
    parser.add_argument(
        "--manifest-path",
        type=Path,
        default=DEFAULT_MANIFEST_PATH,
        help="Path to Cargo.toml for the PyO3 crate.",
    )
    parser.add_argument(
        "--interpreter",
        nargs="+",
        help="Python interpreter path(s) passed to `maturin build --interpreter`.",
    )
    parser.add_argument(
        "--skip-wheel",
        action="store_true",
        help="Skip wheel build.",
    )
    parser.add_argument(
        "--skip-sdist",
        action="store_true",
        help="Skip sdist build.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print commands without executing them.",
    )
    return parser.parse_args()


def build_commands(args: argparse.Namespace) -> List[List[str]]:
    commands: List[List[str]] = []

    if not args.skip_wheel:
        wheel_command = [
            sys.executable,
            "-m",
            "maturin",
            "build",
            "--release",
            "--manifest-path",
            str(args.manifest_path),
            "--out",
            str(args.out_dir),
        ]
        if args.interpreter:
            wheel_command.extend(["--interpreter", *args.interpreter])
        commands.append(wheel_command)

    if not args.skip_sdist:
        sdist_command = [
            sys.executable,
            "-m",
            "maturin",
            "sdist",
            "--manifest-path",
            str(args.manifest_path),
            "--out",
            str(args.out_dir),
        ]
        commands.append(sdist_command)

    return commands


def merged_environment() -> Dict[str, str]:
    env = os.environ.copy()
    env.setdefault(ABI3_FORWARD_COMPAT_ENV, "1")
    return env


def command_to_string(command: Sequence[str]) -> str:
    return " ".join(shlex.quote(part) for part in command)


def run_commands(
    commands: Sequence[Sequence[str]],
    *,
    dry_run: bool,
    env: Optional[Dict[str, str]] = None,
) -> int:
    for command in commands:
        rendered_command = command_to_string(command)
        print(f"$ {rendered_command}")
        if dry_run:
            continue

        run = subprocess.run(command, env=env)
        if run.returncode != 0:
            return run.returncode

    return 0


def main() -> int:
    args = parse_args()

    if args.skip_wheel and args.skip_sdist:
        print("At least one artifact type must be enabled.", file=sys.stderr)
        return 2

    args.out_dir.mkdir(parents=True, exist_ok=True)
    commands = build_commands(args)

    env = merged_environment()
    print(f"Using {ABI3_FORWARD_COMPAT_ENV}={env.get(ABI3_FORWARD_COMPAT_ENV, '')}")
    if args.dry_run:
        print("Dry run enabled; no artifacts will be built.")

    return run_commands(commands, dry_run=args.dry_run, env=env)


if __name__ == "__main__":
    raise SystemExit(main())
