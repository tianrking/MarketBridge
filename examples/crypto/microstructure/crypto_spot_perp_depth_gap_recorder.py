#!/usr/bin/env python3
"""Append spot/perp target-size depth observations to a JSONL archive."""

import argparse
import json
import time
from pathlib import Path

from crypto_spot_perp_depth_gap_monitor import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--target-notional", type=float, default=10_000.0)
    parser.add_argument("--top-levels", type=int, default=10)
    parser.add_argument("--min-depth-ratio", type=float, default=2.0)
    parser.add_argument("--min-impact-improvement-bps", type=float, default=5.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-spot-perp-depth.jsonl"))
    args = parser.parse_args()
    if (args.target_notional <= 0 or args.top_levels <= 0 or args.min_depth_ratio < 1.0
            or args.min_impact_improvement_bps < 0 or args.iterations <= 0
            or args.interval_secs < 0):
        parser.error("invalid target, depth, threshold, iteration or interval arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "symbol": args.symbol, "exchange": args.exchange,
        "target_notional": args.target_notional, "top_levels": args.top_levels,
        "min_depth_ratio": args.min_depth_ratio,
        "min_impact_improvement_bps": args.min_impact_improvement_bps,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            observation = observe(
                args.base_url, args.symbol, args.exchange, args.target_notional,
                args.top_levels, args.min_depth_ratio,
                args.min_impact_improvement_bps, args.timeout,
            )
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": observation,
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({
        "strategy": "crypto_spot_perp_depth_gap_recorder",
        "output": str(args.output),
        "appended_snapshots": args.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
