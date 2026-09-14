#!/usr/bin/env python3
"""Freeze public liquidation history beside a synchronized MarketBridge quote."""

import argparse
import json
import time
from pathlib import Path

from crypto_microstructure_monitor import fetch


def quote_price(payload, symbol, exchange, product_type):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
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
    parser.add_argument("--exchange", choices=("okx", "coinex"), default="okx")
    parser.add_argument("--price-exchange", choices=("okx", "binance"), default=None)
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--product-type", choices=("spot", "perp"), default="perp")
    parser.add_argument("--liquidation-limit", type=int, default=100)
    parser.add_argument("--window-hours", type=float, default=24.0)
    parser.add_argument("--threshold-notional", type=float, default=1_000_000_000.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=60.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-liquidation-burst-response.jsonl"))
    args = parser.parse_args()
    if (args.liquidation_limit <= 0 or args.liquidation_limit > 100
            or args.window_hours <= 0 or args.threshold_notional < 0
            or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid liquidation, window, threshold, iteration, interval or timeout arguments")
    price_exchange = args.price_exchange or args.exchange
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "exchange": args.exchange, "price_exchange": price_exchange,
        "symbol": args.symbol.upper(), "product_type": args.product_type,
        "liquidation_limit": args.liquidation_limit,
        "window_hours": args.window_hours, "threshold_notional": args.threshold_notional,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            liquidation_payload = fetch(args.base_url, "/v1/history/liquidations", {
                "exchange": args.exchange, "symbol": args.symbol,
                "limit": args.liquidation_limit,
            }, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.symbol, "exchanges": price_exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            error = liquidation_payload.get("error")
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {
                    "liquidations": liquidation_payload.get("rows", []),
                    "price": quote_price(quote_payload, args.symbol, price_exchange,
                                          args.product_type),
                    "research_only": True,
                },
                "coverage": liquidation_payload.get("coverage_detail"),
                "upstream_errors": ([error] if error else []) + quote_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_liquidation_burst_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
