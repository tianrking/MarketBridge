#!/usr/bin/env python3
"""Observe a bounded CryptoPanic news-attention snapshot via MarketBridge."""

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


def summarize_news(signals, min_score, min_items):
    rows = []
    seen = set()
    for row in signals:
        if (str(row.get("source", "")).lower() != "cryptopanic"
                or str(row.get("category", "")).lower() != "news"
                or str(row.get("metric", "")).lower() != "news_item"):
            continue
        url = row.get("url")
        title = row.get("title")
        identity = url or title or row.get("source_instance")
        if identity in seen:
            continue
        seen.add(identity)
        rows.append({
            "title": title,
            "url": url,
            "score": number(row.get("value")),
            "source_time_ms": row.get("source_time_ms"),
            "ts_ms": row.get("ts_ms"),
        })
    scored = [row for row in rows if row["score"] is not None]
    strong = [row for row in scored if abs(row["score"]) >= min_score]
    net_score = sum(row["score"] for row in scored)
    if not rows:
        state = "observe_only_missing_news"
    elif len(strong) >= min_items:
        state = "news_attention_shock"
    else:
        state = "normal_news_activity"
    return {
        "items": rows,
        "item_count": len(rows),
        "scored_item_count": len(scored),
        "strong_item_count": len(strong),
        "net_score": net_score if scored else None,
        "state": state,
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
            price = number(values.get(key))
            if price is not None and price > 0:
                return {"price": price, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
        bid, ask = number(values.get("bid")), number(values.get("ask"))
        if bid is not None and ask is not None and bid > 0 and ask > 0:
            return {"price": (bid + ask) / 2.0, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def observe(base_url, symbol, exchange, product_type, min_score, min_items, timeout):
    news_payload = fetch(base_url, "/v1/external/signals", {
        "sources": "cryptopanic", "categories": "news",
    }, timeout)
    quote_payload = fetch(base_url, "/v1/market/quotes", {
        "symbols": symbol, "exchanges": exchange, "product_type": product_type,
        "include_stale": "false",
    }, timeout)
    news = summarize_news(news_payload.get("signals", []), min_score, min_items)
    price = quote_price(quote_payload, symbol, exchange)
    return {
        "news": news,
        "price": price,
        "evidence": [
            "cryptopanic_news_available" if news["item_count"] else "missing_or_unconfigured_cryptopanic_news",
            "price_snapshot_available" if price else "missing_price_snapshot",
        ],
        "upstream_errors": news_payload.get("errors", []) + quote_payload.get("errors", []),
        "limitations": [
            "news scores are provider vote aggregates, not a causal sentiment model",
            "the bounded feed is not a complete historical news ledger",
            "attention shock is non-directional and does not imply a trade",
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
    parser.add_argument("--min-score", type=float, default=3.0)
    parser.add_argument("--min-items", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.min_score < 0 or args.min_items <= 0 or args.timeout <= 0:
        parser.error("score must be non-negative; min-items and timeout must be positive")
    print(json.dumps({
        "strategy": "crypto_news_attention_monitor",
        "market": {"symbol": args.symbol.upper(), "exchange": args.exchange,
                   "product_type": args.product_type},
        "filters": {"min_score": args.min_score, "min_items": args.min_items},
        **observe(args.base_url, args.symbol, args.exchange, args.product_type,
                  args.min_score, args.min_items, args.timeout),
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
