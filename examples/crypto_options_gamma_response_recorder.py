#!/usr/bin/env python3
"""Record unsigned option-gamma concentration beside a MarketBridge price quote."""

import argparse
import json
import time
from pathlib import Path

from crypto_microstructure_monitor import fetch
from crypto_options_gamma_monitor import observe


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


def errors(payload):
    result = []
    if isinstance(payload.get("error"), str):
        result.append(payload["error"])
    if isinstance(payload.get("errors"), list):
        result.extend(item for item in payload["errors"] if isinstance(item, str))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--atm-band", type=float, default=0.03)
    parser.add_argument("--min-near-share", type=float, default=0.50)
    parser.add_argument("--min-concentration", type=float, default=0.10)
    parser.add_argument("--max-book-fetches", type=int, default=24)
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-options-gamma-response.jsonl"))
    args = parser.parse_args()
    if (args.expiry_days <= 0 or not 0 < args.atm_band < 0.25
            or not 0 <= args.min_near_share <= 1
            or not 0 <= args.min_concentration <= 1 or args.max_book_fetches < 0
            or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid gamma thresholds, iterations, interval or timeout")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"currency": args.currency, "venue": args.venue,
                  "expiry_days": args.expiry_days, "atm_band": args.atm_band,
                  "min_near_share": args.min_near_share,
                  "min_concentration": args.min_concentration,
                  "max_book_fetches": args.max_book_fetches,
                  "price_symbol": args.price_symbol, "exchange": args.exchange,
                  "product_type": args.product_type}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            gamma = observe(args.base_url, args.currency, args.venue, args.expiry_days,
                            args.atm_band, args.min_near_share, args.min_concentration,
                            args.max_book_fetches, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            price = quote_price(quote_payload, args.price_symbol, args.exchange,
                                args.product_type)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {
                    "gamma": gamma,
                    "price": price,
                    "evidence": [
                        "gamma_snapshot_available" if gamma.get("target_expiry") else "missing_gamma_snapshot",
                        "price_snapshot_available" if price else "missing_price_snapshot",
                    ],
                    "research_only": True,
                },
                "upstream_errors": gamma.get("upstream_errors", []) + errors(quote_payload),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_options_gamma_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
