#!/usr/bin/env python3
"""Record MarketBridge aggregate regime context beside a price snapshot."""

import argparse
import json
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def quote_price(payload, symbol, exchange, product_type):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()):
            continue
        values = row.get("payload") or {}
        if str(instrument.get("product_type", product_type)).lower() != product_type.lower():
            continue
        for key in ("mark", "mid", "price", "last"):
            value = number(values.get(key))
            if value is not None and value > 0:
                return {"price": value, "ts_ms": (row.get("freshness") or {}).get("ts_source"),
                        "stale": bool((row.get("freshness") or {}).get("stale", False))}
        bid, ask = number(values.get("bid")), number(values.get("ask"))
        if bid is not None and ask is not None and bid > 0 and ask > 0:
            return {"price": (bid + ask) / 2.0, "ts_ms": (row.get("freshness") or {}).get("ts_source"),
                    "stale": bool((row.get("freshness") or {}).get("stale", False))}
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default="BTCUSDT,ETHUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--intervals", default="1h,4h,1d")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--price-product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-market-regime.jsonl"))
    args = parser.parse_args()
    if args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0:
        parser.error("iterations, interval and timeout must be valid")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"symbols": args.symbols, "exchange": args.exchange, "market": args.market,
                  "intervals": args.intervals, "price_symbol": args.price_symbol,
                  "price_product_type": args.price_product_type}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            regime_payload = fetch(args.base_url, "/v1/research/market-regime", {
                "symbols": args.symbols, "exchange": args.exchange,
                "market": args.market, "intervals": args.intervals,
            }, args.timeout)
            price_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.exchange,
                "product_type": args.price_product_type, "include_stale": "false",
            }, args.timeout)
            snapshot = regime_payload.get("snapshot") or {}
            price = quote_price(price_payload, args.price_symbol, args.exchange,
                                args.price_product_type)
            observation = {
                "snapshot": snapshot,
                "regime": snapshot.get("regime", "observe_only_unknown_regime"),
                "price": price,
                "evidence": [
                    "market_regime_snapshot_available" if snapshot.get("rows") else "missing_market_regime_snapshot",
                    "price_snapshot_available" if price else "missing_price_snapshot",
                ],
                "upstream_errors": regime_payload.get("errors", []) + price_payload.get("errors", []),
                "research_only": True,
            }
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000),
                                     "iteration": iteration + 1, "parameters": parameters,
                                     "observation": observation}, ensure_ascii=False,
                                    sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_market_regime_recorder", "output": str(args.output),
                      "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
