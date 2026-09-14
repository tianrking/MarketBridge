#!/usr/bin/env python3
"""Freeze Coinbase premium states beside the reference BTC quote."""

import argparse
import json
import time
from pathlib import Path

from crypto_coinbase_premium_monitor import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--coinbase-symbol", default="BTC-USD")
    parser.add_argument("--reference-symbol", default="BTCUSDT")
    parser.add_argument("--reference-exchange", default="binance")
    parser.add_argument("--premium-threshold-bps", type=float, default=5.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=60.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-coinbase-premium-response.jsonl"))
    args = parser.parse_args()
    if (args.premium_threshold_bps < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid threshold, iterations, interval or timeout argument")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "coinbase_symbol": args.coinbase_symbol.upper(),
        "reference_symbol": args.reference_symbol.upper(),
        "reference_exchange": args.reference_exchange.lower(),
        "premium_threshold_bps": args.premium_threshold_bps,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            observation = observe(
                args.base_url, args.coinbase_symbol, args.reference_symbol,
                args.reference_exchange, args.premium_threshold_bps, args.timeout,
            )
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters, "observation": observation,
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_coinbase_premium_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
