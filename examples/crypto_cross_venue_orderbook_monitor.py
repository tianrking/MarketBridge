#!/usr/bin/env python3
"""Measure paper cross-venue order-book edges without executing trades."""

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


def levels(rows):
    parsed = []
    for row in rows or []:
        if isinstance(row, dict):
            price, qty = number(row.get("price")), number(row.get("qty"))
        elif isinstance(row, (list, tuple)) and len(row) >= 2:
            price, qty = number(row[0]), number(row[1])
        else:
            continue
        if price is not None and qty is not None and price > 0 and qty > 0:
            parsed.append((price, qty))
    return parsed


def executable_vwap(rows, target_notional):
    remaining = target_notional
    total_notional = 0.0
    total_qty = 0.0
    for price, qty in rows:
        consumed_notional = min(remaining, price * qty)
        total_notional += consumed_notional
        total_qty += consumed_notional / price
        remaining -= consumed_notional
        if remaining <= 1e-9:
            return total_notional / total_qty, total_qty, total_notional
    return None, total_qty, total_notional


def normalize_books(books):
    normalized = {}
    for row in books:
        exchange = str(row.get("exchange", "")).lower()
        symbol = str(row.get("symbol", "")).upper()
        if not exchange or not symbol:
            instrument = row.get("instrument_ref") or {}
            source = row.get("source_ref") or {}
            exchange = str(source.get("source", "")).lower()
            symbol = str(instrument.get("symbol", "")).upper()
        if exchange and symbol:
            normalized[(exchange, symbol)] = {
                "exchange": exchange,
                "symbol": symbol,
                "bids": levels(row.get("bids")),
                "asks": levels(row.get("asks")),
                "ts_ms": row.get("ts_ms"),
                "stale": bool(row.get("stale", False)),
            }
    return normalized


def summarize_books(books, target_notional, max_skew_ms, paper_cost_bps, min_net_edge_bps):
    normalized = normalize_books(books)
    opportunities = []
    for (buy_exchange, symbol), buy in normalized.items():
        for (sell_exchange, sell_symbol), sell in normalized.items():
            if buy_exchange == sell_exchange or symbol != sell_symbol:
                continue
            buy_vwap, buy_qty, _ = executable_vwap(buy["asks"], target_notional)
            sell_vwap, sell_qty, _ = executable_vwap(sell["bids"], target_notional)
            if buy_vwap is None or sell_vwap is None:
                continue
            timestamps = [buy["ts_ms"], sell["ts_ms"]]
            if not all(isinstance(value, int) for value in timestamps):
                continue
            skew_ms = abs(timestamps[0] - timestamps[1])
            if skew_ms > max_skew_ms:
                continue
            gross_edge_bps = (sell_vwap / buy_vwap - 1.0) * 10_000.0
            net_edge_bps = gross_edge_bps - paper_cost_bps
            opportunities.append({
                "symbol": symbol,
                "buy_exchange": buy_exchange,
                "sell_exchange": sell_exchange,
                "buy_vwap": buy_vwap,
                "sell_vwap": sell_vwap,
                "buy_qty": buy_qty,
                "sell_qty": sell_qty,
                "target_notional": target_notional,
                "gross_edge_bps": gross_edge_bps,
                "paper_cost_bps": paper_cost_bps,
                "net_edge_bps": net_edge_bps,
                "timestamp_skew_ms": skew_ms,
            })
    opportunities.sort(key=lambda row: row["net_edge_bps"], reverse=True)
    best = opportunities[0] if opportunities else None
    state = "observe_only_no_synchronized_depth" if best is None else (
        "cross_venue_book_edge" if best["net_edge_bps"] >= min_net_edge_bps
        else "cross_venue_edge_below_paper_hurdle"
    )
    return {
        "state": state,
        "best_opportunity": best,
        "opportunities": opportunities,
        "book_count": len(normalized),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchanges", default="binance,okx,bybit")
    parser.add_argument("--market", default="spot", choices=("spot", "perp"))
    parser.add_argument("--target-notional", type=float, default=10_000.0)
    parser.add_argument("--max-skew-ms", type=int, default=2_000)
    parser.add_argument("--paper-cost-bps", type=float, default=20.0)
    parser.add_argument("--min-net-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.target_notional <= 0 or args.max_skew_ms < 0 or args.paper_cost_bps < 0
            or args.timeout <= 0):
        parser.error("target, skew, paper cost and timeout arguments are invalid")
    payload = fetch(args.base_url, "/v1/market/order-books", {
        "market": args.market, "symbols": args.symbol, "exchanges": args.exchanges,
    }, args.timeout)
    summary = summarize_books(
        payload.get("books", []), args.target_notional, args.max_skew_ms,
        args.paper_cost_bps, args.min_net_edge_bps,
    )
    print(json.dumps({
        "strategy": "crypto_cross_venue_orderbook_monitor",
        "market": {"symbol": args.symbol.upper(), "product_type": args.market,
                   "exchanges": args.exchanges},
        "filters": {"target_notional": args.target_notional,
                    "max_skew_ms": args.max_skew_ms,
                    "paper_cost_bps": args.paper_cost_bps,
                    "min_net_edge_bps": args.min_net_edge_bps},
        "summary": summary,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "books are snapshots and are not synchronized fills or queue positions",
            "paper cost is a sensitivity input, not venue fees, transfer cost or slippage",
            "prefunded inventory, settlement latency, borrow, counterparty and execution are absent",
            "no order, wallet, transfer or allocation path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
