#!/usr/bin/env python3
"""Record option put/call open-interest composition to JSONL."""

import argparse
import json
import time
from pathlib import Path

from crypto_options_put_call_oi_monitor import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--max-expiry-days", type=float, default=180.0)
    parser.add_argument("--high-ratio", type=float, default=1.0)
    parser.add_argument("--low-ratio", type=float, default=0.6)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-options-put-call-oi.jsonl"))
    args = parser.parse_args()
    if (args.max_expiry_days <= 0 or args.high_ratio <= args.low_ratio
            or args.low_ratio < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid expiry, ratio, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"currency": args.currency, "venue": args.venue,
                  "max_expiry_days": args.max_expiry_days,
                  "high_ratio": args.high_ratio, "low_ratio": args.low_ratio}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            observation = observe(args.base_url, args.currency, args.venue,
                                  args.max_expiry_days, args.high_ratio,
                                  args.low_ratio, args.timeout)
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000),
                                     "iteration": iteration + 1,
                                     "parameters": parameters,
                                     "observation": observation},
                                    ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_options_put_call_oi_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
