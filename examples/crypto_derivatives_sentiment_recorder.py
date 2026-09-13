#!/usr/bin/env python3
"""Record read-only aggregate derivatives sentiment snapshots to JSONL."""

import argparse
import json
import sys
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_derivatives_sentiment_monitor import summarize_signals  # noqa: E402


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/external/signals?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTC")
    parser.add_argument("--long-short-high", type=float, default=1.2)
    parser.add_argument("--long-short-low", type=float, default=0.8)
    parser.add_argument("--liquidation-threshold", type=float, default=1_000_000.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-derivatives-sentiment.jsonl"))
    args = parser.parse_args()
    if (args.long_short_low < 0 or args.long_short_high <= args.long_short_low
            or args.liquidation_threshold < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid thresholds, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "symbol": args.symbol, "long_short_high": args.long_short_high,
        "long_short_low": args.long_short_low,
        "liquidation_threshold": args.liquidation_threshold,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            payload = fetch(args.base_url, {
                "sources": "coinglass", "symbols": args.symbol,
                "metrics": "funding_rate,long_short_ratio,open_interest,basis,liquidation,options_open_interest",
            }, args.timeout)
            observation = summarize_signals(
                payload.get("signals", []), args.long_short_high,
                args.long_short_low, args.liquidation_threshold,
            )
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": observation,
                "upstream_errors": payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({
        "strategy": "crypto_derivatives_sentiment_recorder",
        "output": str(args.output),
        "appended_snapshots": args.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
