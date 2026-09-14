#!/usr/bin/env python3
"""Append read-only bull-call-spread option snapshots to JSONL."""

import argparse
import json
import time
from pathlib import Path

from crypto_options_bull_call_spread_monitor import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--long-moneyness", type=float, default=0.95)
    parser.add_argument("--short-moneyness", type=float, default=1.05)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-options-bull-call-spread.jsonl"))
    options = parser.parse_args()
    if (options.expiry_days <= 0 or options.long_moneyness <= 0
            or options.short_moneyness <= options.long_moneyness or options.iterations <= 0
            or options.interval_secs < 0):
        parser.error("invalid expiry, moneyness, iteration or interval arguments")
    options.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"currency": options.currency, "venue": options.venue,
                  "expiry_days": options.expiry_days, "long_moneyness": options.long_moneyness,
                  "short_moneyness": options.short_moneyness}
    with options.output.open("a", encoding="utf-8") as handle:
        for iteration in range(options.iterations):
            observation = observe(options.base_url, options.currency, options.venue,
                                  options.expiry_days, options.long_moneyness,
                                  options.short_moneyness, options.timeout)
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000),
                                     "iteration": iteration + 1, "parameters": parameters,
                                     "observation": observation}, ensure_ascii=False,
                                    sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < options.iterations:
                time.sleep(options.interval_secs)
    print(json.dumps({"strategy": "crypto_options_bull_call_spread_recorder",
                      "output": str(options.output), "appended_snapshots": options.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
