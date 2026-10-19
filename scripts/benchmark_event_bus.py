"""Run repeatable, lossless in-process profiles on Linux, macOS and Windows.

Build with cargo build --release --locked first. Results include binary hashes,
host/toolchain metadata and raw per-run reports; they are not network capacity.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]


def command_output(command):
    try:
        return subprocess.check_output(command, cwd=ROOT, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release" / ("market-bridge.exe" if os.name == "nt" else "market-bridge"))
    parser.add_argument("--output", type=Path, required=True, help="new JSONL file; existing files are never overwritten")
    parser.add_argument("--label", required=True, help="implementation/build label")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--events", type=int, default=20_000)
    parser.add_argument("--symbols", type=int, default=1024)
    parser.add_argument("--subscribers", type=int, default=8)
    parser.add_argument("--workers", type=int, default=4,
                        help="Tokio workers; the load producer also uses the main thread outside this pool")
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()
    if not (1 <= args.runs <= 100 and 0 <= args.warmups <= 10 and 1 <= args.events <= 1_048_576
            and 1 <= args.symbols <= 100_000 and 1 <= args.subscribers <= 256
            and 1 <= args.workers <= 256 and 0 < args.timeout <= 3600):
        parser.error("invalid run counts, events, symbols, subscribers, workers or timeout")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"binary not found: {binary}")
    capacity = 1 << (args.events - 1).bit_length()
    environment = dict(os.environ, TOKIO_WORKER_THREADS=str(args.workers))
    cases = [
        ("quotes-small", "quotes", 2, 1),
        ("quotes-wide", "quotes", args.symbols, 1),
        ("trades-wide", "trades", args.symbols, 1),
        ("books-wide", "books", args.symbols, 1),
        ("mixed-wide", "mixed", args.symbols, 1),
        ("quotes-wide-sharded", "quotes", args.symbols, 4),
    ]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256()
    with binary.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    # Exclusive creation protects previous results, including interrupted runs.
    with args.output.open("x", encoding="utf-8") as output:
        metadata = {
            "kind": "metadata", "label": args.label, "binary_sha256": digest.hexdigest(),
            "checkout_revision": command_output(["git", "rev-parse", "HEAD"]),
            "checkout_dirty": bool(command_output(["git", "status", "--porcelain"])),
            "platform": platform.platform(), "machine": platform.machine(),
            "rustc": command_output(["rustc", "--version"]),
            "tokio_worker_threads": args.workers,
            "producer_thread": "main block_on thread, outside the Tokio worker pool",
            "events": args.events, "subscribers": args.subscribers,
            "broadcast_capacity_per_shard": capacity, "book_levels_per_side": 20,
            "runs_per_case": args.runs, "warmups_per_case": args.warmups,
            "scope": "EventBus and latest snapshots; no source/router/network/storage/serialization load",
        }
        output.write(json.dumps(metadata) + "\n")
        output.flush()
        for name, profile, symbols, shards in cases:
            command = [str(binary), "load-test", "--events", str(args.events),
                       "--subscribers", str(args.subscribers), "--broadcast-capacity", str(capacity),
                       "--profile", profile, "--symbols", str(symbols), "--book-levels", "20",
                       "--event-bus-shards", str(shards)]
            for run in range(-args.warmups, args.runs):
                started = time.perf_counter()
                try:
                    result = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True,
                                            text=True, timeout=args.timeout)
                except subprocess.TimeoutExpired:
                    print(f"FAIL {name}: exceeded {args.timeout}s", file=sys.stderr)
                    return 1
                elapsed = time.perf_counter() - started
                try:
                    report = json.loads(result.stdout)
                except json.JSONDecodeError:
                    print(f"FAIL {name}: invalid report\n{result.stderr}", file=sys.stderr)
                    return result.returncode or 1
                record = {"kind": "warmup" if run < 0 else "measurement", "case": name,
                          "run": run + 1, "command": command[1:], "process_elapsed_seconds": elapsed,
                          "exit_code": result.returncode, "report": report}
                output.write(json.dumps(record) + "\n")
                output.flush()
                if result.returncode or not report.get("passed") or not report.get("lossless"):
                    print(f"FAIL {name}: {result.stderr.strip()}; see {args.output}", file=sys.stderr)
                    return result.returncode or 1
                print(f"PASS {args.label} {name} run {run + 1}: "
                      f"{report['publish_events_per_sec']:.0f} events/s; "
                      f"publish p99 {report['bus_publish_call_latency_us']['p99']:.3f} us", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
