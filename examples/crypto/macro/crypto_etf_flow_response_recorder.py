#!/usr/bin/env python3
"""Record MarketBridge Farside ETF flow signals beside a BTC quote."""

import argparse
import json
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


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


def signal_row(payload, asset):
    for row in payload.get("signals", []):
        if (str(row.get("source", "")).lower() == "farside_etf"
                and str(row.get("symbol", "")).upper() == asset.upper()
                and row.get("value") is not None):
            raw = row.get("raw") or {}
            return {"flow_musd": float(row["value"]),
                    "source_time_ms": row.get("source_time_ms"),
                    "date": raw.get("date"), "raw": raw}
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--asset", default="BTC")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=900.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-etf-flow-response.jsonl"))
    args = parser.parse_args()
    if args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0:
        parser.error("iterations, interval and timeout must be valid")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            signal_payload = fetch(args.base_url, "/v1/external/signals", {
                "sources": "farside_etf", "symbols": args.asset,
            }, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.symbol, "exchanges": args.exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            flow = signal_row(signal_payload, args.asset)
            price = quote_price(quote_payload, args.symbol, args.exchange, args.product_type)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": {"asset": args.asset, "symbol": args.symbol,
                               "exchange": args.exchange, "product_type": args.product_type},
                "observation": {"flow": flow, "price": price, "research_only": True},
                "upstream_errors": signal_payload.get("errors", []) + quote_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_etf_flow_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
