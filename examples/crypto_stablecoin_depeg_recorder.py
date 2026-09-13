#!/usr/bin/env python3
"""Record stablecoin depeg-risk snapshots to JSONL."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_stablecoin_depeg_monitor import fetch, summarize_quotes  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="spot", choices=("spot", "dex_pool"))
    parser.add_argument("--stable-symbols", default="USDTUSDC,USDCUSDT,DAIUSDT")
    parser.add_argument("--risk-symbol", default="BTCUSDT")
    parser.add_argument("--watch-bps", type=float, default=20.0)
    parser.add_argument("--stress-bps", type=float, default=50.0)
    parser.add_argument("--max-spread-bps", type=float, default=40.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-stablecoin-depeg.jsonl"))
    args = parser.parse_args()
    if (args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0
            or args.watch_bps < 0 or args.stress_bps < args.watch_bps or args.max_spread_bps < 0):
        parser.error("invalid thresholds, iterations, interval or timeout")
    symbols = ",".join(dict.fromkeys(
        item.strip().upper() for item in f"{args.stable_symbols},{args.risk_symbol}".split(",") if item.strip()
    ))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"exchange": args.exchange.lower(), "product_type": args.product_type,
                  "stable_symbols": symbols, "risk_symbol": args.risk_symbol.upper(),
                  "watch_bps": args.watch_bps, "stress_bps": args.stress_bps,
                  "max_spread_bps": args.max_spread_bps}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": symbols, "exchanges": args.exchange, "product_type": args.product_type,
                "include_stale": "false",
            }, args.timeout)
            observation = summarize_quotes(
                payload.get("quotes", []), args.risk_symbol, args.exchange,
                args.watch_bps, args.stress_bps, args.max_spread_bps,
            )
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters, "observation": observation,
                "upstream_errors": payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_stablecoin_depeg_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
