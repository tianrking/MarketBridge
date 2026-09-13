#!/usr/bin/env python3
"""Record footprint pressure beside a synchronized MarketBridge quote."""

import argparse
import json
import time
from pathlib import Path

from crypto_footprint_imbalance_monitor import fetch, summarize_footprints


def quote_price(payload, symbol, exchange, product_type):
    for row in payload.get("quotes", []):
        instrument, source = row.get("instrument_ref") or {}, row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()
                or str(instrument.get("product_type", product_type)).lower() != product_type.lower()):
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
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp", choices=("spot", "perp"))
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
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
    parser.add_argument("--output", type=Path, default=Path("work/crypto-footprint-response.jsonl"))
    args = parser.parse_args()
    if (args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0
            or args.interval_ms <= 0 or args.scale <= 0 or args.imbalance_ratio <= 0
            or args.imbalance_volume < 0 or args.stacked_imbalance_range <= 0 or args.limit <= 0
            or not 0 < args.min_delta_ratio < 1 or args.min_stacked_levels <= 0):
        parser.error("invalid footprint or recorder arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"exchange": args.exchange.lower(), "market": args.market,
                  "symbol": args.symbol.upper(), "product_type": args.product_type,
                  "interval_ms": args.interval_ms, "scale": args.scale,
                  "imbalance_ratio": args.imbalance_ratio, "imbalance_volume": args.imbalance_volume,
                  "stacked_imbalance_range": args.stacked_imbalance_range,
                  "min_delta_ratio": args.min_delta_ratio, "min_stacked_levels": args.min_stacked_levels}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            footprint_payload = fetch(args.base_url, {
                "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                "interval_ms": args.interval_ms, "scale": args.scale,
                "imbalance_ratio": args.imbalance_ratio, "imbalance_volume": args.imbalance_volume,
                "stacked_imbalance_range": args.stacked_imbalance_range,
                "include_trades": "false", "limit": args.limit,
            }, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.symbol, "exchanges": args.exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            footprint = summarize_footprints(footprint_payload.get("footprints", []),
                                             args.min_delta_ratio, args.min_stacked_levels)
            price = quote_price(quote_payload, args.symbol, args.exchange, args.product_type)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {"footprint": footprint, "price": price, "research_only": True},
                "upstream_errors": footprint_payload.get("errors", []) + quote_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_footprint_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
