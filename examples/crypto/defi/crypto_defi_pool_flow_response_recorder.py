#!/usr/bin/env python3
"""Record DEX-pool pressure beside a synchronized MarketBridge BTC quote."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from crypto_defi_pool_flow_monitor import observe_pools
from crypto_microstructure_monitor import fetch


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
    parser.add_argument("--sources", default=None)
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--min-liquidity-usd", type=float, default=100_000.0)
    parser.add_argument("--min-turnover-h1", type=float, default=0.25)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-defi-pool-flow-response.jsonl"))
    args = parser.parse_args()
    if (args.min_liquidity_usd < 0 or args.min_turnover_h1 < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid liquidity, turnover, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "sources": args.sources, "symbols": args.symbols,
        "min_liquidity_usd": args.min_liquidity_usd, "min_turnover_h1": args.min_turnover_h1,
        "price_exchange": args.price_exchange.lower(), "price_symbol": args.price_symbol.upper(),
        "product_type": args.product_type,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            pools = observe_pools(
                args.base_url, args.sources, args.symbols,
                args.min_liquidity_usd, args.min_turnover_h1, args.timeout,
            )
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.price_exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {
                    "pools": pools.get("pools", []), "pool_count": pools.get("pool_count", 0),
                    "price": quote_price(quote_payload, args.price_symbol,
                                          args.price_exchange, args.product_type),
                    "research_only": True,
                },
                "upstream_errors": pools.get("upstream_errors", []) + quote_payload.get("errors", []),
                "evidence": pools.get("evidence", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_defi_pool_flow_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
