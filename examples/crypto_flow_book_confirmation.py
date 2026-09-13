#!/usr/bin/env python3
"""Confirm or reject a crypto perp order-book pressure hypothesis.

This read-only monitor joins three point-in-time observations: top-of-book
depth, aggressive trade flow (delta/CVD) and perpetual funding.  A large L2
imbalance by itself can disappear or be spoofed, so a directional candidate is
only marked confirmed when the taker-flow imbalance points the same way.  It
never places orders and never treats missing data as zero.
"""

import argparse
import json
import time

from crypto_microstructure_monitor import fetch, funding_context, imbalance


def flow_imbalance(row):
    """Return signed taker-flow ratio and preserve the raw notional evidence."""
    if not isinstance(row, dict):
        return {"ratio": None, "buy_notional": None, "sell_notional": None, "delta_notional": None}
    buy = row.get("buy_notional")
    sell = row.get("sell_notional")
    delta = row.get("delta_notional")
    if not isinstance(buy, (int, float)) or not isinstance(sell, (int, float)):
        return {"ratio": None, "buy_notional": buy, "sell_notional": sell, "delta_notional": delta}
    total = buy + sell
    ratio = (buy - sell) / total if total > 0 else None
    return {
        "ratio": ratio,
        "buy_notional": buy,
        "sell_notional": sell,
        "delta_notional": delta if isinstance(delta, (int, float)) else buy - sell,
        "trade_count": row.get("trade_count"),
        "large_trade_count": row.get("large_trade_count"),
        "bucket_start_ms": row.get("bucket_start_ms"),
        "bucket_end_ms": row.get("bucket_end_ms"),
        "updated_at_ms": row.get("updated_at_ms"),
    }


def classify_confirmation(book_ratio, flow_ratio, threshold, flow_threshold):
    if book_ratio is None:
        return "observe_only_missing_book"
    if flow_ratio is None:
        if abs(book_ratio) >= threshold:
            return "unconfirmed_book_pressure_missing_flow"
        return "observe_only_missing_flow"
    book_side = 1 if book_ratio >= threshold else -1 if book_ratio <= -threshold else 0
    flow_side = 1 if flow_ratio >= flow_threshold else -1 if flow_ratio <= -flow_threshold else 0
    if book_side and flow_side == book_side:
        return "confirmed_bid_pressure" if book_side > 0 else "confirmed_ask_pressure"
    if book_side and flow_side and flow_side != book_side:
        return "book_flow_conflict"
    if book_side:
        return "weak_flow_against_book" if flow_side == -book_side else "unconfirmed_book_pressure_weak_flow"
    if flow_side:
        return "flow_pressure_without_book_confirmation"
    return "balanced_book_and_flow"


def observe(base_url, symbol, exchange, top_levels, threshold, flow_threshold, extreme_pct, window_ms, timeout):
    books_payload = fetch(base_url, "/v1/market/order-books", {
        "market": "perp", "symbols": symbol, "exchanges": exchange,
    }, timeout)
    flow_payload = fetch(base_url, "/v1/market/order-flow", {
        "market": "perp", "symbol": symbol, "exchange": exchange,
        "window_ms": window_ms, "limit": 1,
    }, timeout)
    funding_payload = fetch(base_url, "/v1/market/perpetual-funding", {
        "symbols": symbol, "exchanges": exchange, "active_only": "true", "limit": 100,
    }, timeout)
    book = next((row for row in books_payload.get("books", [])
                 if str(row.get("symbol", "")).upper() == symbol.upper()
                 and str(row.get("exchange", "")).lower() == exchange.lower()), None)
    book_metrics = imbalance(book.get("bids", []), book.get("asks", []), top_levels) if book else None
    flow_row = next((row for row in flow_payload.get("order_flow", [])
                     if str(row.get("symbol", "")).upper() == symbol.upper()), None)
    flow_metrics = flow_imbalance(flow_row)
    funding = funding_context(funding_payload, symbol, extreme_pct)
    signal = classify_confirmation(
        book_metrics.get("imbalance") if book_metrics else None,
        flow_metrics.get("ratio"), threshold, flow_threshold,
    )
    return {
        "symbol": symbol,
        "exchange": exchange,
        "window_ms": window_ms,
        "book": {"ts_ms": book.get("ts_ms") if book else None, "metrics": book_metrics},
        "order_flow": {"row_available": flow_row is not None, "metrics": flow_metrics},
        "funding": funding,
        "signal": signal,
        "upstream_errors": (books_payload.get("errors", [])
                            + flow_payload.get("errors", [])
                            + funding_payload.get("errors", [])),
        "evidence": [
            "order_book_snapshot_available" if book else "missing_order_book_snapshot",
            "order_flow_bucket_available" if flow_row else "missing_order_flow_bucket",
            "funding_row_available" if funding["row"] else "missing_funding_row",
        ],
        "limitations": [
            "L2 depth is a snapshot and can disappear before any fill",
            "taker-flow delta is an observed window, not a causal price forecast",
            "funding is crowding context; no fees, slippage, queue, latency or liquidation model",
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
    parser.add_argument("--flow-threshold", type=float, default=0.20)
    parser.add_argument("--funding-extreme-pct", type=float, default=0.01)
    parser.add_argument("--window-ms", type=int, default=60000)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.top_levels <= 0 or not 0.0 < options.imbalance_threshold < 1.0
            or not 0.0 < options.flow_threshold < 1.0 or options.funding_extreme_pct < 0
            or options.window_ms <= 0 or options.iterations <= 0 or options.interval_secs < 0):
        parser.error("invalid levels, thresholds, window, iteration or interval arguments")
    for iteration in range(options.iterations):
        result = observe(options.base_url, options.symbol, options.exchange,
                         options.top_levels, options.imbalance_threshold, options.flow_threshold,
                         options.funding_extreme_pct, options.window_ms, options.timeout)
        print(json.dumps({"strategy": "crypto_flow_book_confirmation",
                          "iteration": iteration + 1, **result}, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
