#!/usr/bin/env python3
"""Record cross-venue book-edge states beside a synchronized BTC quote."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_cross_venue_orderbook_monitor import fetch, summarize_books  # noqa: E402


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
            value = values.get(key)
            if isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0:
                return {"price": float(value), "ts_ms": (row.get("freshness") or {}).get("ts_source")}
        bid, ask = values.get("bid"), values.get("ask")
        if (isinstance(bid, (int, float)) and isinstance(ask, (int, float))
                and bid > 0 and ask > 0):
            return {"price": (bid + ask) / 2.0,
                    "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


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
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--price-product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=5.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-cross-venue-orderbook-response.jsonl"))
    args = parser.parse_args()
    if (args.target_notional <= 0 or args.max_skew_ms < 0 or args.paper_cost_bps < 0
            or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid target, skew, cost, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "symbol": args.symbol.upper(), "exchanges": args.exchanges, "market": args.market,
        "target_notional": args.target_notional, "max_skew_ms": args.max_skew_ms,
        "paper_cost_bps": args.paper_cost_bps, "min_net_edge_bps": args.min_net_edge_bps,
        "price_exchange": args.price_exchange.lower(), "price_symbol": args.price_symbol.upper(),
        "price_product_type": args.price_product_type,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            books_payload = fetch(args.base_url, "/v1/market/order-books", {
                "market": args.market, "symbols": args.symbol, "exchanges": args.exchanges,
            }, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.price_exchange,
                "product_type": args.price_product_type, "include_stale": "false",
            }, args.timeout)
            observation = summarize_books(
                books_payload.get("books", []), args.target_notional, args.max_skew_ms,
                args.paper_cost_bps, args.min_net_edge_bps,
            )
            observation["price"] = quote_price(quote_payload, args.price_symbol,
                                                args.price_exchange, args.price_product_type)
            observation["research_only"] = True
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters, "observation": observation,
                "upstream_errors": books_payload.get("errors", []) + quote_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_cross_venue_orderbook_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
