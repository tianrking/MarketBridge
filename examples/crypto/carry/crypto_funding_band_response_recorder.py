#!/usr/bin/env python3
"""Record provider funding-band proximity beside a synchronized BTC quote."""

import argparse
import json
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_funding_band_monitor import fetch, observe


def fetch_endpoint(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


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
            value = number(values.get(key))
            if value is not None and value > 0:
                return {"price": value, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
        bid, ask = number(values.get("bid")), number(values.get("ask"))
        if bid is not None and ask is not None and bid > 0 and ask > 0:
            return {"price": (bid + ask) / 2.0,
                    "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def aggregate_state(rows):
    states = {row.get("state") for row in rows}
    if "near_upper_funding_cap" in states:
        return "near_upper_funding_cap"
    if "near_lower_funding_floor" in states:
        return "near_lower_funding_floor"
    if "within_provider_funding_band" in states:
        return "within_provider_funding_band"
    return "observe_only_missing_provider_band"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--threshold", type=float, default=0.8)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-funding-band-response.jsonl"))
    args = parser.parse_args()
    if not 0 < args.threshold <= 1 or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0:
        parser.error("threshold, iterations, interval and timeout must be valid")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "symbol": args.symbol.upper(), "exchange": args.exchange.lower(),
        "threshold": args.threshold, "price_exchange": args.price_exchange.lower(),
        "price_symbol": args.price_symbol.upper(), "product_type": args.product_type,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            funding_payload = fetch(args.base_url, {
                "exchange": args.exchange, "symbols": args.symbol,
                "active_only": "true", "limit": 100,
            }, args.timeout)
            quote_payload = fetch_endpoint(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.price_exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            observation = observe(funding_payload, args.symbol, args.threshold)
            rows = observation.get("rows", [])
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {
                    "funding_rows": rows, "state": aggregate_state(rows),
                    "price": quote_price(quote_payload, args.price_symbol,
                                          args.price_exchange, args.product_type),
                    "research_only": True,
                },
                "upstream_errors": funding_payload.get("errors", [])
                + quote_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_funding_band_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
