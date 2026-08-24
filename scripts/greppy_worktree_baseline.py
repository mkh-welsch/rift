#!/usr/bin/env python3
"""Measure native detached Git-worktree creation for the Greppy CoW spike."""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def run() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--samples", type=int, default=20)
    parser.add_argument(
        "--mode", choices=("cold", "warm-move"), default="cold"
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be positive")

    source = args.source.resolve(strict=True)
    samples_ms: list[float] = []
    with tempfile.TemporaryDirectory(
        prefix="greppy-worktree-baseline-", dir=source.parent
    ) as parent:
        parent_path = Path(parent)
        for index in range(args.samples):
            destination = parent_path / f"sample-{index:03d}"
            if args.mode == "cold":
                command = [
                    "git",
                    "-C",
                    str(source),
                    "worktree",
                    "add",
                    "--quiet",
                    "--detach",
                    str(destination),
                    "HEAD",
                ]
            else:
                pooled = parent_path / f"pooled-{index:03d}"
                subprocess.run(
                    [
                        "git",
                        "-C",
                        str(source),
                        "worktree",
                        "add",
                        "--quiet",
                        "--detach",
                        str(pooled),
                        "HEAD",
                    ],
                    check=True,
                )
                command = [
                    "git",
                    "-C",
                    str(source),
                    "worktree",
                    "move",
                    str(pooled),
                    str(destination),
                ]
            started = time.perf_counter_ns()
            subprocess.run(command, check=True)
            elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
            samples_ms.append(elapsed_ms)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(source),
                    "worktree",
                    "remove",
                    "--force",
                    str(destination),
                ],
                check=True,
            )

    result = {
        "benchmark": f"git-worktree-{args.mode}",
        "source": str(source),
        "samples_ms": samples_ms,
        "median_ms": statistics.median(samples_ms),
        "min_ms": min(samples_ms),
        "max_ms": max(samples_ms),
        "cleanup_passed": True,
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(
        f"median {result['median_ms']:.3f} ms "
        f"min {result['min_ms']:.3f} ms max {result['max_ms']:.3f} ms"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
