#!/usr/bin/env python3
"""Record options skew beside a synchronized MarketBridge BTC quote."""

import argparse
import json
import time
from pathlib import Path

from crypto_microstructure_monitor import fetch
from crypto_options_skew_monitor import observe


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
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--atm-band", type=float, default=0.03)
    parser.add_argument("--wing-min", type=float, default=0.85)
    parser.add_argument("--wing-max", type=float, default=1.15)
    parser.add_argument("--min-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-term-slope-iv", type=float, default=3.0)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-options-skew-response.jsonl"))
    args = parser.parse_args()
    if (args.expiry_days <= 0 or not 0 < args.atm_band < 0.25
            or not 0 < args.wing_min < 1 or args.wing_max <= 1
            or args.wing_min >= args.wing_max or args.min_skew_iv < 0
            or args.min_term_slope_iv < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid expiry, moneyness, threshold, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "currency": args.currency.upper(), "venue": args.venue,
        "expiry_days": args.expiry_days, "atm_band": args.atm_band,
        "wing_min": args.wing_min, "wing_max": args.wing_max,
        "min_skew_iv": args.min_skew_iv, "min_term_slope_iv": args.min_term_slope_iv,
        "price_exchange": args.price_exchange.lower(), "price_symbol": args.price_symbol.upper(),
        "product_type": args.product_type,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            options = observe(args.base_url, args.currency, args.venue, args.expiry_days,
                              args.atm_band, args.wing_min, args.wing_max,
                              args.min_skew_iv, args.min_term_slope_iv, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.price_exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {
                    "target_expiry": options.get("target_expiry"),
                    "term_structure": options.get("term_structure"),
                    "price": quote_price(quote_payload, args.price_symbol,
                                          args.price_exchange, args.product_type),
                    "research_only": True,
                },
                "upstream_errors": options.get("upstream_errors", []) + quote_payload.get("errors", []),
                "evidence": options.get("evidence", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_options_skew_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
