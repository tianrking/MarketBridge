#!/usr/bin/env python3
"""Record triangular quote edges beside a synchronized MarketBridge BTC quote."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_triangular_arbitrage_monitor import fetch, summarize_quotes  # noqa: E402


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
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--start-notional", type=float, default=10_000.0)
    parser.add_argument("--max-skew-ms", type=int, default=500)
    parser.add_argument("--paper-cost-bps-per-leg", type=float, default=10.0)
    parser.add_argument("--min-net-edge-bps", type=float, default=0.0)
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--price-product-type", default="spot", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=2.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-triangular-arbitrage-response.jsonl"))
    args = parser.parse_args()
    if (args.start_notional <= 0 or args.max_skew_ms < 0
            or args.paper_cost_bps_per_leg < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid notional, skew, cost, iteration, interval or timeout")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "exchange": args.exchange.lower(), "start_notional": args.start_notional,
        "max_skew_ms": args.max_skew_ms, "paper_cost_bps_per_leg": args.paper_cost_bps_per_leg,
        "min_net_edge_bps": args.min_net_edge_bps, "price_symbol": args.price_symbol.upper(),
        "price_product_type": args.price_product_type,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": "BTCUSDT,ETHBTC,ETHUSDT", "exchanges": args.exchange,
                "product_type": "spot", "include_stale": "false",
            }, args.timeout)
            price_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.exchange,
                "product_type": args.price_product_type, "include_stale": "false",
            }, args.timeout)
            observation = summarize_quotes(
                payload.get("quotes", []), args.exchange, args.start_notional,
                args.paper_cost_bps_per_leg, args.max_skew_ms, args.min_net_edge_bps,
            )
            observation["price"] = quote_price(price_payload, args.price_symbol,
                                                args.exchange, args.price_product_type)
            observation["research_only"] = True
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters, "observation": observation,
                "upstream_errors": payload.get("errors", []) + price_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_triangular_arbitrage_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
