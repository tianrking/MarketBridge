#!/usr/bin/env python3
"""Observe Coinbase spot premium relative to a reference spot quote.

The falsifiable hypothesis is deliberately narrow: when the public Coinbase
spot quote is materially above or below a reference venue, does the next
fixed-record BTC response differ from ordinary observations?  A USD/USDT quote
unit difference, venue coverage, and timestamp alignment remain explicit.
This observer never treats a premium as an order or a guaranteed US spot-flow
measure.
"""

import argparse
import json
import math
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        value = float(value)
    except (TypeError, ValueError):
        return None
    return value if math.isfinite(value) else None


def quote_symbol(row):
    instrument = row.get("instrument_ref") or {}
    return str(instrument.get("symbol") or row.get("symbol") or "").upper()


def quote_price(row):
    values = row.get("payload") or {}
    for key in ("mid", "mark", "price", "last"):
        value = number(values.get(key))
        if value is not None and value > 0:
            return {"price": value, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    bid, ask = number(values.get("bid")), number(values.get("ask"))
    if bid is not None and ask is not None and bid > 0 and ask > 0:
        return {"price": (bid + ask) / 2.0,
                "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def find_quote(payload, symbol, exchange):
    for row in payload.get("quotes", []):
        source = row.get("source_ref") or {}
        instrument = row.get("instrument_ref") or {}
        if (str(source.get("source", "")).lower() != exchange.lower()
                or str(instrument.get("product_type", "spot")).lower() != "spot"
                or quote_symbol(row) != symbol.upper()):
            continue
        quote = quote_price(row)
        if quote is not None:
            return quote
    return None


def classify_premium(premium_bps, threshold_bps):
    if premium_bps is None:
        return "observe_only_missing_premium_alignment"
    if premium_bps >= threshold_bps:
        return "coinbase_premium"
    if premium_bps <= -threshold_bps:
        return "coinbase_discount"
    return "ordinary_coinbase_reference_spread"


def observe(base_url, coinbase_symbol, reference_symbol, reference_exchange,
            threshold_bps, timeout):
    payload = fetch(base_url, "/v1/market/quotes", {
        "exchanges": f"coinbase,{reference_exchange}",
        "product_type": "spot", "include_stale": "false",
    }, timeout)
    coinbase = find_quote(payload, coinbase_symbol, "coinbase")
    reference = find_quote(payload, reference_symbol, reference_exchange)
    premium_bps = None
    if coinbase and reference and coinbase["price"] > 0 and reference["price"] > 0:
        premium_bps = math.log(coinbase["price"] / reference["price"]) * 10_000.0
    return {
        "coinbase_symbol": coinbase_symbol,
        "reference_symbol": reference_symbol,
        "reference_exchange": reference_exchange,
        "coinbase_quote": coinbase,
        "reference_quote": reference,
        "premium_bps": premium_bps,
        "state": classify_premium(premium_bps, threshold_bps),
        "thresholds": {"premium_threshold_bps": threshold_bps},
        "evidence": [
            "coinbase_spot_quote_available" if coinbase else "missing_coinbase_spot_quote",
            "reference_spot_quote_available" if reference else "missing_reference_spot_quote",
            "premium_alignment_available" if premium_bps is not None else "missing_premium_alignment",
        ],
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "USD versus USDT quote-unit differences can contaminate the spread",
            "quotes are snapshots and do not prove US spot flow or executable conversion",
            "no fees, latency, transfer, inventory, fill or execution model",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--coinbase-symbol", default="BTC-USD")
    parser.add_argument("--reference-symbol", default="BTCUSDT")
    parser.add_argument("--reference-exchange", default="binance")
    parser.add_argument("--premium-threshold-bps", type=float, default=5.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.premium_threshold_bps < 0 or args.timeout <= 0:
        parser.error("premium threshold must be non-negative and timeout positive")
    result = observe(args.base_url, args.coinbase_symbol, args.reference_symbol,
                     args.reference_exchange, args.premium_threshold_bps, args.timeout)
    print(json.dumps({"strategy": "crypto_coinbase_premium_monitor",
                      "observed_at_ms": int(time.time() * 1000), **result},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
