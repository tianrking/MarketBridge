#!/usr/bin/env python3
"""Record footprint imbalance snapshots to JSONL."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_footprint_imbalance_monitor import fetch, summarize_footprints  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp", choices=("spot", "perp"))
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--interval-ms", type=int, default=60_000)
    parser.add_argument("--scale", type=float, default=1.0)
    parser.add_argument("--imbalance-ratio", type=float, default=3.0)
    parser.add_argument("--imbalance-volume", type=float, default=0.0)
    parser.add_argument("--stacked-imbalance-range", type=int, default=3)
    parser.add_argument("--limit", type=int, default=20)
    parser.add_argument("--min-delta-ratio", type=float, default=0.20)
    parser.add_argument("--min-stacked-levels", type=int, default=1)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-footprint-imbalance.jsonl"))
    args = parser.parse_args()
    if (args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0
            or args.interval_ms <= 0 or args.scale <= 0 or args.imbalance_ratio <= 0
            or args.imbalance_volume < 0 or args.stacked_imbalance_range <= 0 or args.limit <= 0
            or not 0 < args.min_delta_ratio < 1 or args.min_stacked_levels <= 0):
        parser.error("invalid footprint or recorder arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"exchange": args.exchange.lower(), "market": args.market, "symbol": args.symbol.upper(),
                  "interval_ms": args.interval_ms, "scale": args.scale,
                  "imbalance_ratio": args.imbalance_ratio, "imbalance_volume": args.imbalance_volume,
                  "stacked_imbalance_range": args.stacked_imbalance_range,
                  "min_delta_ratio": args.min_delta_ratio, "min_stacked_levels": args.min_stacked_levels}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            payload = fetch(args.base_url, {"exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                                            "interval_ms": args.interval_ms, "scale": args.scale,
                                            "imbalance_ratio": args.imbalance_ratio,
                                            "imbalance_volume": args.imbalance_volume,
                                            "stacked_imbalance_range": args.stacked_imbalance_range,
                                            "include_trades": "false", "limit": args.limit}, args.timeout)
            observation = summarize_footprints(payload.get("footprints", []), args.min_delta_ratio,
                                               args.min_stacked_levels)
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                                     "parameters": parameters, "observation": observation,
                                     "upstream_errors": payload.get("errors", [])},
                                    ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_footprint_imbalance_recorder", "output": str(args.output),
                      "appended_snapshots": args.iterations, "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
