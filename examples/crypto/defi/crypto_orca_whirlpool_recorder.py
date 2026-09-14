#!/usr/bin/env python3
"""Record read-only Orca Whirlpool states to JSONL."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_orca_whirlpool_monitor import observe_pools  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--min-turnover-ratio", type=float, default=1.0)
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--interval-secs", type=float, default=60.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-orca-whirlpool.jsonl"))
    args = parser.parse_args()
    if (args.min_turnover_ratio < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid turnover, iteration, interval or timeout argument")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"symbols": args.symbols, "min_turnover_ratio": args.min_turnover_ratio}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            observation = observe_pools(args.base_url, args.symbols,
                                        args.min_turnover_ratio, args.timeout)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": observation,
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_orca_whirlpool_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
