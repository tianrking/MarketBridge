#!/usr/bin/env python3
"""Freeze order-book imbalance and funding context beside a quote."""

import argparse
import json
import time
from pathlib import Path

from crypto_microstructure_monitor import fetch, observe


def quote_price(payload, symbol, exchange):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()
                or str(instrument.get("product_type", "perp")).lower() != "perp"):
            continue
        values = row.get("payload") or {}
        for key in ("mark", "mid", "price", "last"):
            value = values.get(key)
            if isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0:
                return {"price": float(value), "ts_ms": (row.get("freshness") or {}).get("ts_source")}
        bid, ask = values.get("bid"), values.get("ask")
        if (isinstance(bid, (int, float)) and isinstance(ask, (int, float))
                and bid > 0 and ask > 0):
            return {"price": (bid + ask) / 2.0,
                    "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--top-levels", type=int, default=5)
    parser.add_argument("--imbalance-threshold", type=float, default=0.30)
    parser.add_argument("--funding-extreme-pct", type=float, default=0.01)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-microstructure-response.jsonl"))
    args = parser.parse_args()
    if (args.top_levels <= 0 or not 0.0 < args.imbalance_threshold < 1.0
            or args.funding_extreme_pct < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid depth, threshold, iteration, interval or timeout argument")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"symbol": args.symbol, "exchange": args.exchange,
                  "top_levels": args.top_levels, "imbalance_threshold": args.imbalance_threshold,
                  "funding_extreme_pct": args.funding_extreme_pct}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            observation = observe(args.base_url, args.symbol, args.exchange,
                                  args.top_levels, args.imbalance_threshold,
                                  args.funding_extreme_pct, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.symbol, "exchanges": args.exchange,
                "product_type": "perp", "include_stale": "false",
            }, args.timeout)
            observation["response_price"] = quote_price(quote_payload, args.symbol, args.exchange)
            observation["upstream_errors"] = list(observation.get("upstream_errors", []))
            observation["upstream_errors"].extend(quote_payload.get("errors", []))
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters, "observation": observation,
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_microstructure_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
