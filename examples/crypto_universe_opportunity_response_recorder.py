#!/usr/bin/env python3
"""Freeze universe candidates beside candidate and BTC perpetual quotes."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_universe_opportunity_recorder import observe  # noqa: E402
from crypto_universe_opportunity_scan import fetch  # noqa: E402


def quote_prices(payload, exchange):
    prices = {}
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(source.get("source", "")).lower() != exchange.lower()
                or str(instrument.get("product_type", "perp")).lower() != "perp"):
            continue
        symbol = str(instrument.get("symbol", "")).upper()
        if not symbol:
            continue
        values = row.get("payload") or {}
        price = None
        for key in ("mark", "mid", "price", "last"):
            value = values.get(key)
            if isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0:
                price = float(value)
                break
        if price is None:
            bid, ask = values.get("bid"), values.get("ask")
            if (isinstance(bid, (int, float)) and isinstance(ask, (int, float))
                    and bid > 0 and ask > 0):
                price = (bid + ask) / 2.0
        if price is not None:
            prices[symbol] = {
                "price": price,
                "ts_ms": (row.get("freshness") or {}).get("ts_source"),
            }
    return prices


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--symbols", default="")
    parser.add_argument("--benchmark-symbol", default="BTCUSDT")
    parser.add_argument("--price-top-k", type=int, default=3)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--min-quote-volume", type=float, default=1_000_000.0)
    parser.add_argument("--min-realized-vol", type=float, default=0.0)
    parser.add_argument("--min-abs-funding-hourly-pct", type=float, default=0.01)
    parser.add_argument("--min-score", type=int, default=2)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-universe-opportunity-response.jsonl"))
    args = parser.parse_args()
    if (args.price_top_k <= 0 or args.limit <= 0 or args.min_quote_volume < 0
            or args.min_realized_vol < 0 or args.min_abs_funding_hourly_pct < 0
            or not 0 <= args.min_score <= 3 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid top-k, limits, thresholds, score, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "exchange": args.exchange, "market": args.market, "interval": args.interval,
        "symbols": sorted(item.strip().upper() for item in args.symbols.split(",") if item.strip()),
        "benchmark_symbol": args.benchmark_symbol.upper(), "price_top_k": args.price_top_k,
        "limit": args.limit, "min_quote_volume": args.min_quote_volume,
        "min_realized_vol": args.min_realized_vol,
        "min_abs_funding_hourly_pct": args.min_abs_funding_hourly_pct,
        "min_score": args.min_score,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            observation = observe(
                args.base_url, args.exchange, args.market, args.interval, args.symbols,
                args.limit, args.min_quote_volume, args.min_realized_vol,
                args.min_abs_funding_hourly_pct, args.min_score, args.timeout,
            )
            selected = [str(row.get("symbol", "")).upper()
                        for row in observation.get("candidates", [])[:args.price_top_k]
                        if row.get("symbol")]
            symbols = ",".join(dict.fromkeys(selected + [args.benchmark_symbol.upper()]))
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": symbols, "exchanges": args.exchange,
                "product_type": "perp", "include_stale": "false",
            }, args.timeout)
            observation["response_prices"] = quote_prices(quote_payload, args.exchange)
            observation["benchmark_symbol"] = args.benchmark_symbol.upper()
            observation["upstream_errors"] = list(observation.get("upstream_errors", []))
            observation["upstream_errors"].extend(quote_payload.get("errors", []))
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters, "observation": observation,
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_universe_opportunity_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
