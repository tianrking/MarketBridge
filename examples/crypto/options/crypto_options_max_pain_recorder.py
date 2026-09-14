#!/usr/bin/env python3
"""Record expiry-level max-pain proxies to JSONL."""

import argparse
import json
import time
from pathlib import Path

from crypto_options_max_pain_monitor import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--max-expiry-days", type=float, default=180.0)
    parser.add_argument("--near-expiry-days", type=float, default=3.0)
    parser.add_argument("--near-distance-pct", type=float, default=2.0)
    parser.add_argument("--min-oi", type=float, default=0.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-options-max-pain.jsonl"))
    args = parser.parse_args()
    if (args.max_expiry_days <= 0 or args.near_expiry_days < 0
            or args.near_expiry_days > args.max_expiry_days or args.near_distance_pct < 0
            or args.min_oi < 0 or args.iterations <= 0 or args.interval_secs < 0
            or args.timeout <= 0):
        parser.error("invalid expiry, distance, OI, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"currency": args.currency, "venue": args.venue,
                  "max_expiry_days": args.max_expiry_days,
                  "near_expiry_days": args.near_expiry_days,
                  "near_distance_pct": args.near_distance_pct, "min_oi": args.min_oi}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            observation = observe(args.base_url, args.currency, args.venue,
                                  args.max_expiry_days, args.near_expiry_days,
                                  args.near_distance_pct, args.min_oi, args.timeout)
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000),
                                     "iteration": iteration + 1,
                                     "parameters": parameters,
                                     "observation": observation},
                                    ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_options_max_pain_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
