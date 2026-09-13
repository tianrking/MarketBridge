#!/usr/bin/env python3
"""Monitor perp order-book imbalance with funding context.

This is a read-only implementation of a public crypto microstructure narrative:
stronger top-of-book bid depth may indicate short-term buy pressure, while an
extreme funding rate is a crowding warning rather than confirmation.  The
monitor reports both pieces of evidence and their conflict; it never places
orders or treats a missing book as zero imbalance.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def depth_notional(levels, top_levels):
    total = 0.0
    for level in levels[:top_levels]:
        price = level.get("price")
        qty = level.get("qty")
        if isinstance(price, (int, float)) and isinstance(qty, (int, float)) and price > 0 and qty > 0:
            total += price * qty
    return total


def imbalance(bids, asks, top_levels):
    bid_depth = depth_notional(bids, top_levels)
    ask_depth = depth_notional(asks, top_levels)
    total = bid_depth + ask_depth
    return {
        "bid_depth_notional": bid_depth,
        "ask_depth_notional": ask_depth,
        "imbalance": (bid_depth - ask_depth) / total if total > 0 else None,
        "top_levels": top_levels,
    }


def funding_context(payload, symbol, extreme_pct):
    row = next(
        (
            row
            for row in payload.get("funding", [])
            if str(row.get("symbol", "")).upper() == symbol.upper()
        ),
        None,
    )
    if not row:
        return {"row": None, "state": "missing_funding"}
    rate_pct = row.get("funding_rate_pct")
    state = "normal"
    if isinstance(rate_pct, (int, float)) and rate_pct >= extreme_pct:
        state = "long_crowded_warning"
    elif isinstance(rate_pct, (int, float)) and rate_pct <= -extreme_pct:
        state = "short_crowded_warning"
    return {
        "row": {
            "exchange": row.get("exchange"),
            "symbol": row.get("symbol"),
            "funding_rate_pct": rate_pct,
            "funding_interval_ms": row.get("funding_interval_ms"),
            "next_funding_time_ms": row.get("next_funding_time_ms"),
            "mark_price": row.get("mark_price"),
            "ts_ms": row.get("ts_ms"),
        },
        "state": state,
    }


def classify_signal(book_imbalance, funding_state, threshold):
    if book_imbalance is None:
        return "observe_only_missing_book"
    if book_imbalance >= threshold:
        if funding_state == "long_crowded_warning":
            return "bid_pressure_with_long_crowding_conflict"
        return "bid_pressure_candidate"
    if book_imbalance <= -threshold:
        if funding_state == "short_crowded_warning":
            return "ask_pressure_with_short_crowding_conflict"
        return "ask_pressure_candidate"
    return "balanced_book"


def observe(base_url, symbol, exchange, top_levels, threshold, extreme_pct, timeout):
    books_payload = fetch(base_url, "/v1/market/order-books", {
        "market": "perp",
        "symbols": symbol,
        "exchanges": exchange,
    }, timeout)
    funding_payload = fetch(base_url, "/v1/market/perpetual-funding", {
        "symbols": symbol,
        "exchanges": exchange,
        "active_only": "true",
        "limit": 100,
    }, timeout)
    book = next(
        (
            row for row in books_payload.get("books", [])
            if str(row.get("symbol", "")).upper() == symbol.upper()
            and str(row.get("exchange", "")).lower() == exchange.lower()
        ),
        None,
    )
    book_metrics = imbalance(book.get("bids", []), book.get("asks", []), top_levels) if book else None
    funding = funding_context(funding_payload, symbol, extreme_pct)
    signal = classify_signal(
        book_metrics.get("imbalance") if book_metrics else None,
        funding["state"],
        threshold,
    )
    return {
        "symbol": symbol,
        "exchange": exchange,
        "book": {
            "ts_ms": book.get("ts_ms") if book else None,
            "metrics": book_metrics,
        },
        "funding": funding,
        "signal": signal,
        "upstream_errors": funding_payload.get("errors", []),
        "evidence": [
            "order_book_snapshot_available" if book else "missing_order_book_snapshot",
            "funding_row_available" if funding["row"] else "missing_funding_row",
        ],
        "limitations": [
            "depth imbalance is a snapshot and can disappear before any fill",
            "funding context is crowding evidence, not a directional guarantee",
            "no queue position, trade-through, latency, fees, slippage or liquidation model",
        ],
        "execution": "research_only_no_orders",
    }


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
    options = parser.parse_args()
    if (
        options.top_levels <= 0
        or not 0.0 < options.imbalance_threshold < 1.0
        or options.funding_extreme_pct < 0
        or options.iterations <= 0
        or options.interval_secs < 0
    ):
        parser.error("invalid depth, threshold, iteration or interval arguments")
    for iteration in range(options.iterations):
        result = observe(
            options.base_url,
            options.symbol,
            options.exchange,
            options.top_levels,
            options.imbalance_threshold,
            options.funding_extreme_pct,
            options.timeout,
        )
        print(json.dumps({
            "strategy": "crypto_microstructure_monitor",
            "iteration": iteration + 1,
            **result,
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
