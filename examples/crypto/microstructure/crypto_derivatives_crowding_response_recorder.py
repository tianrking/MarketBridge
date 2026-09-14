#!/usr/bin/env python3
"""Record aggregate derivatives crowding and a synchronized price snapshot.

The recorder turns a public crowding/reversal narrative into a frozen sample:
CoinGlass aggregate funding/OI/long-short/liquidation context is recorded next
to a MarketBridge quote.  It never interprets the snapshot as ownership and
never sends an order.
"""

import argparse
import json
import sys
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sentiment"))
from crypto_derivatives_sentiment_monitor import summarize_signals  # noqa: E402
from crypto_sentiment_extremes_monitor import quote_price  # noqa: E402


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTC")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--long-short-high", type=float, default=1.2)
    parser.add_argument("--long-short-low", type=float, default=0.8)
    parser.add_argument("--liquidation-threshold", type=float, default=1_000_000.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-derivatives-crowding-response.jsonl"))
    args = parser.parse_args()
    if (args.long_short_low < 0 or args.long_short_high <= args.long_short_low
            or args.liquidation_threshold < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid thresholds, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "symbol": args.symbol,
        "price_symbol": args.price_symbol,
        "exchange": args.exchange,
        "product_type": args.product_type,
        "long_short_high": args.long_short_high,
        "long_short_low": args.long_short_low,
        "liquidation_threshold": args.liquidation_threshold,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            signal_payload = fetch(args.base_url, "/v1/external/signals", {
                "sources": "coinglass", "symbols": args.symbol,
                "metrics": "funding_rate,long_short_ratio,open_interest,basis,liquidation,options_open_interest",
            }, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            summary = summarize_signals(
                signal_payload.get("signals", []), args.long_short_high,
                args.long_short_low, args.liquidation_threshold,
            )
            price = quote_price(quote_payload, args.price_symbol, args.exchange)
            observation = {
                "summary": summary,
                "price": price,
                "evidence": [
                    "coinglass_positioning_available" if summary["state"] != "observe_only_missing_positioning_metrics"
                    else "missing_positioning_metrics",
                    "price_snapshot_available" if price else "missing_price_snapshot",
                ],
                "upstream_errors": signal_payload.get("errors", []) + quote_payload.get("errors", []),
                "research_only": True,
            }
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
        "strategy": "crypto_derivatives_crowding_response_recorder",
        "output": str(args.output),
        "appended_snapshots": args.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
