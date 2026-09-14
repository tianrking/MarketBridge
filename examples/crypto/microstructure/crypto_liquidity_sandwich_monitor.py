#!/usr/bin/env python3
"""Observe two-sided order-book walls as a bounded liquidity-sandwich context.

The public X lead describes a narrow, falsifiable observation: when both sides
of a BTC book show meaningful near-touch depth while the spread stays tight,
does the next fixed-record BTC move differ from ordinary snapshots?  This
module only classifies the current book.  It does not infer intent, persistence,
price direction, executable fills, or a range-trading instruction.
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


def levels(raw):
    result = []
    for level in raw or []:
        if not isinstance(level, dict):
            continue
        price = number(level.get("price"))
        quantity = number(level.get("qty", level.get("quantity", level.get("size"))))
        if price is not None and quantity is not None and price > 0 and quantity > 0:
            result.append((price, quantity))
    return result


def side_depth(levels_, reference, side, band_bps):
    if reference is None or reference <= 0 or band_bps < 0:
        return None
    total = 0.0
    count = 0
    for price, quantity in levels_:
        distance_bps = ((reference - price) if side == "bid" else (price - reference)) / reference * 10_000.0
        if distance_bps < 0 or distance_bps > band_bps:
            continue
        total += price * quantity
        count += 1
    return {"notional": total, "levels": count}


def classify_sandwich(metrics, min_side_depth_notional, min_symmetry_ratio,
                      max_spread_bps):
    if metrics is None:
        return "observe_only_missing_order_book"
    bid = metrics.get("bid_depth_notional")
    ask = metrics.get("ask_depth_notional")
    spread = metrics.get("spread_bps")
    if bid is None or ask is None or spread is None:
        return "observe_only_missing_near_touch_metrics"
    if min(bid, ask) < min_side_depth_notional:
        return "ordinary_book_context"
    symmetry = min(bid, ask) / max(bid, ask) if max(bid, ask) > 0 else 0.0
    if symmetry >= min_symmetry_ratio and spread <= max_spread_bps + 1e-9:
        return "liquidity_sandwich"
    return "ordinary_book_context"


def book_metrics(book, depth_band_bps):
    if not book:
        return None
    bids = levels(book.get("bids"))
    asks = levels(book.get("asks"))
    if not bids or not asks:
        return None
    best_bid, best_ask = bids[0][0], asks[0][0]
    midpoint = (best_bid + best_ask) / 2.0
    if midpoint <= 0 or best_ask < best_bid:
        return None
    bid_depth = side_depth(bids, midpoint, "bid", depth_band_bps)
    ask_depth = side_depth(asks, midpoint, "ask", depth_band_bps)
    if bid_depth is None or ask_depth is None:
        return None
    bid_notional, ask_notional = bid_depth["notional"], ask_depth["notional"]
    symmetry = (min(bid_notional, ask_notional) / max(bid_notional, ask_notional)
                if max(bid_notional, ask_notional) > 0 else None)
    return {
        "best_bid": best_bid,
        "best_ask": best_ask,
        "midpoint": midpoint,
        "spread_bps": (best_ask - best_bid) / midpoint * 10_000.0,
        "bid_depth_notional": bid_notional,
        "ask_depth_notional": ask_notional,
        "bid_levels": bid_depth["levels"],
        "ask_levels": ask_depth["levels"],
        "depth_symmetry_ratio": symmetry,
        "depth_band_bps": depth_band_bps,
    }


def observe(base_url, symbol, exchange, depth_band_bps,
            min_side_depth_notional, min_symmetry_ratio, max_spread_bps, timeout):
    payload = fetch(base_url, "/v1/market/order-books", {
        "market": "perp", "symbols": symbol, "exchanges": exchange,
    }, timeout)
    book = next((row for row in payload.get("books", [])
                 if str(row.get("symbol", "")).upper() == symbol.upper()
                 and str(row.get("exchange", "")).lower() == exchange.lower()), None)
    metrics = book_metrics(book, depth_band_bps)
    state = classify_sandwich(metrics, min_side_depth_notional,
                               min_symmetry_ratio, max_spread_bps)
    return {
        "symbol": symbol,
        "exchange": exchange,
        "state": state,
        "book": {"ts_ms": book.get("ts_ms") if book else None, "metrics": metrics},
        "thresholds": {
            "depth_band_bps": depth_band_bps,
            "min_side_depth_notional": min_side_depth_notional,
            "min_symmetry_ratio": min_symmetry_ratio,
            "max_spread_bps": max_spread_bps,
        },
        "evidence": [
            "two_sided_near_touch_depth_available" if metrics else "missing_near_touch_depth",
            "narrow_spread" if metrics and metrics["spread_bps"] <= max_spread_bps
            else "spread_not_narrow_or_missing",
        ],
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "displayed depth is a snapshot and not a fill or persistence guarantee",
            "two-sided depth symmetry does not prove a range, intent or price direction",
            "no fees, latency, queue position, cancellation or execution model",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--depth-band-bps", type=float, default=10.0)
    parser.add_argument("--min-side-depth-notional", type=float, default=100_000.0)
    parser.add_argument("--min-symmetry-ratio", type=float, default=0.5)
    parser.add_argument("--max-spread-bps", type=float, default=2.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.depth_band_bps < 0 or args.min_side_depth_notional < 0
            or not 0 <= args.min_symmetry_ratio <= 1 or args.max_spread_bps < 0
            or args.timeout <= 0):
        parser.error("depth band, depth threshold, symmetry, spread and timeout must be valid")
    result = observe(args.base_url, args.symbol, args.exchange, args.depth_band_bps,
                     args.min_side_depth_notional, args.min_symmetry_ratio,
                     args.max_spread_bps, args.timeout)
    print(json.dumps({"strategy": "crypto_liquidity_sandwich_monitor",
                      "observed_at_ms": int(time.time() * 1000), **result},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
