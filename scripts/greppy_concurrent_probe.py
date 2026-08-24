#!/usr/bin/env python3
"""Create exact Rift snapshots concurrently and verify lifecycle cleanup."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import subprocess
import time
import uuid
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("source", type=Path)
    parser.add_argument("--workers", type=int, default=10)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.workers < 1:
        parser.error("--workers must be positive")

    binary = args.binary.resolve(strict=True)
    source = args.source.resolve(strict=True)
    prefix = f"greppy-concurrent-{uuid.uuid4().hex[:12]}"

    def create(index: int) -> dict[str, object]:
        started = time.perf_counter_ns()
        completed = subprocess.run(
            [
                str(binary),
                "create",
                str(source),
                "--name",
                f"{prefix}-{index:02d}",
                "--copy-all",
                "--no-hooks",
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        return {
            "path": completed.stdout.strip(),
            "elapsed_ms": (time.perf_counter_ns() - started) / 1_000_000,
        }

    started = time.perf_counter_ns()
    created: list[dict[str, object]] = []
    create_phase_ms = 0.0
    cleanup_started = 0
    try:
        with concurrent.futures.ThreadPoolExecutor(
            max_workers=args.workers
        ) as executor:
            futures = [executor.submit(create, index) for index in range(args.workers)]
            for future in concurrent.futures.as_completed(futures):
                created.append(future.result())
        create_phase_ms = (time.perf_counter_ns() - started) / 1_000_000
    finally:
        cleanup_started = time.perf_counter_ns()
        for result in created:
            subprocess.run(
                [str(binary), "remove", str(result["path"]), "--no-hooks"],
                check=True,
            )
        subprocess.run([str(binary), "gc"], check=True)

    total_ms = (time.perf_counter_ns() - started) / 1_000_000
    result = {
        "benchmark": "rift-concurrent-exact-create",
        "source": str(source),
        "workers": args.workers,
        "create_phase_ms": create_phase_ms,
        "cleanup_phase_ms": (time.perf_counter_ns() - cleanup_started) / 1_000_000,
        "total_ms_including_cleanup": total_ms,
        "creates": created,
        "cleanup_passed": all(not Path(str(item["path"])).exists() for item in created),
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    if not result["cleanup_passed"]:
        raise SystemExit("cleanup verification failed")
    print(
        f"{args.workers} concurrent exact snapshots passed; "
        f"total including cleanup {total_ms:.3f} ms"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
