#!/usr/bin/env python3
"""Observe crypto Fear & Greed extremes with a MarketBridge price snapshot.

This is a read-only context observer.  It labels an extreme sentiment state
and keeps the current price alongside it; it does not predict a return or
place an order.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def summarize_signals(signals, fear_max, greed_min):
    row = next(
        (
            row for row in signals
            if str(row.get("source", "")).lower() == "fear_greed"
            and str(row.get("metric", "")).lower() == "fear_greed_index"
        ),
        None,
    )
    value = number(row.get("value")) if row else None
    raw = row.get("raw") if row else None
    classification = raw.get("classification") if isinstance(raw, dict) else None
    if value is None or not 0 <= value <= 100:
        state = "observe_only_missing_or_invalid_sentiment"
    elif value <= fear_max:
        state = "extreme_fear_context"
    elif value >= greed_min:
        state = "extreme_greed_context"
    else:
        state = "neutral_sentiment_context"
    return {
        "value": value,
        "classification": classification,
        "state": state,
        "source_ts_ms": row.get("source_time_ms") if row else None,
        "signal_ts_ms": row.get("ts_ms") if row else None,
    }


def quote_price(payload, symbol, exchange):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if str(instrument.get("symbol", "")).upper() != symbol.upper():
            continue
        if str(source.get("source", "")).lower() != exchange.lower():
            continue
        values = row.get("payload") or {}
        for key in ("mark", "mid", "price", "last"):
            value = number(values.get(key))
            if value is not None and value > 0:
                return {
                    "price": value,
                    "ts_ms": (row.get("freshness") or {}).get("ts_source"),
                    "stale": bool((row.get("freshness") or {}).get("stale", False)),
                }
        bid = number(values.get("bid"))
        ask = number(values.get("ask"))
        if bid is not None and ask is not None and bid > 0 and ask > 0:
            return {
                "price": (bid + ask) / 2.0,
                "ts_ms": (row.get("freshness") or {}).get("ts_source"),
                "stale": bool((row.get("freshness") or {}).get("stale", False)),
            }
    return None


def observe(base_url, symbol, exchange, product_type, fear_max, greed_min, timeout):
    sentiment_payload = fetch(base_url, "/v1/external/signals", {
        "sources": "fear_greed", "metrics": "fear_greed_index",
    }, timeout)
    quote_payload = fetch(base_url, "/v1/market/quotes", {
        "symbols": symbol, "exchanges": exchange, "product_type": product_type,
        "include_stale": "false",
    }, timeout)
    sentiment = summarize_signals(sentiment_payload.get("signals", []), fear_max, greed_min)
    price = quote_price(quote_payload, symbol, exchange)
    return {
        "sentiment": sentiment,
        "price": price,
        "evidence": [
            "fear_greed_signal_available" if sentiment["value"] is not None
            else "missing_or_invalid_fear_greed_signal",
            "price_snapshot_available" if price else "missing_price_snapshot",
        ],
        "upstream_errors": sentiment_payload.get("errors", []) + quote_payload.get("errors", []),
        "limitations": [
            "Fear and Greed is a provider composite, not a position ledger or causal factor",
            "the snapshot is not a synchronized historical observation until recorded",
            "no allocation, wallet, order or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="spot", choices=("spot", "perp"))
    parser.add_argument("--fear-max", type=float, default=20.0)
    parser.add_argument("--greed-min", type=float, default=80.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if not 0 <= args.fear_max < args.greed_min <= 100 or args.timeout <= 0:
        parser.error("sentiment thresholds must satisfy 0 <= fear-max < greed-min <= 100")
    print(json.dumps({
        "strategy": "crypto_sentiment_extremes_monitor",
        "market": {"symbol": args.symbol.upper(), "exchange": args.exchange,
                   "product_type": args.product_type},
        "filters": {"fear_max": args.fear_max, "greed_min": args.greed_min},
        **observe(args.base_url, args.symbol, args.exchange, args.product_type,
                  args.fear_max, args.greed_min, args.timeout),
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
