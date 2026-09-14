#!/usr/bin/env python3
"""Record put/call open-interest state beside a BTC quote."""

import argparse
import json
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_options_put_call_oi_monitor import observe


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
        if isinstance(bid, (int, float)) and isinstance(ask, (int, float)) and bid > 0 and ask > 0:
            return {"price": (bid + ask) / 2.0, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--max-expiry-days", type=float, default=180.0)
    parser.add_argument("--high-ratio", type=float, default=1.0)
    parser.add_argument("--low-ratio", type=float, default=0.6)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-options-put-call-oi-response.jsonl"))
    args = parser.parse_args()
    if (args.max_expiry_days <= 0 or args.high_ratio <= args.low_ratio
            or args.low_ratio < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid expiry, ratio, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"currency": args.currency, "venue": args.venue,
                  "price_exchange": args.price_exchange, "price_symbol": args.price_symbol,
                  "product_type": args.product_type, "max_expiry_days": args.max_expiry_days,
                  "high_ratio": args.high_ratio, "low_ratio": args.low_ratio}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            oi = observe(args.base_url, args.currency, args.venue, args.max_expiry_days,
                         args.high_ratio, args.low_ratio, args.timeout)
            quotes = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.price_exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000),
                                     "iteration": iteration + 1,
                                     "parameters": parameters,
                                     "observation": {"oi": oi,
                                                     "price": quote_price(quotes, args.price_symbol,
                                                                           args.price_exchange,
                                                                           args.product_type)},
                                     "upstream_errors": oi.get("upstream_errors", []) + quotes.get("errors", [])},
                                    ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_options_put_call_oi_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
