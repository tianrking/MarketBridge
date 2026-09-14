#!/usr/bin/env python3
"""Record perpetual open-interest impulses beside a MarketBridge quote."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "strategy"))
from python_strategy_runner import fetch  # noqa: E402


def latest_open_interest(payload, exchange):
    rows = payload.get("open_interest")
    if not isinstance(rows, list):
        rows = payload.get("rows", [])
    candidates = [row for row in rows if isinstance(row, dict)
                  and (not exchange or str(row.get("exchange", "")).lower() == exchange.lower())]
    if not candidates:
        return None
    row = candidates[-1]
    value = row.get("open_interest")
    if not isinstance(value, (int, float)) or isinstance(value, bool) or value <= 0:
        return None
    return {"value": float(value), "ts_ms": row.get("ts_ms") or row.get("timestamp")}


def quote_price(payload, symbol, exchange):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()):
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


def payload_errors(payload):
    errors = []
    if isinstance(payload.get("error"), str):
        errors.append(payload["error"])
    if isinstance(payload.get("errors"), list):
        errors.extend(item for item in payload["errors"] if isinstance(item, str))
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--min-oi-change-pct", type=float, default=0.25)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-oi-impulse-response.jsonl"))
    args = parser.parse_args()
    if (args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0
            or args.min_oi_change_pct < 0):
        parser.error("iterations, timeout and OI threshold must be valid")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    previous_oi = None
    previous_price = None
    client = lambda path, params: fetch(args.base_url, path, params, args.timeout)
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            oi_payload = client("/v1/market/open-interest", {"symbols": args.symbol})
            quote_payload = client("/v1/market/quotes", {
                "symbols": args.symbol, "exchanges": args.exchange,
                "product_type": "perp", "include_stale": "false",
            })
            oi = latest_open_interest(oi_payload, args.exchange)
            quote = quote_price(quote_payload, args.symbol, args.exchange)
            oi_change = ((oi["value"] / previous_oi - 1.0) * 100.0
                         if oi is not None and previous_oi and previous_oi > 0 else None)
            price = quote.get("price") if quote else None
            price_change = ((price / previous_price - 1.0) * 100.0
                            if price is not None and previous_price and previous_price > 0 else None)
            if oi is not None:
                previous_oi = oi["value"]
            if price is not None:
                previous_price = price
            state = ("oi_expansion" if oi_change is not None and oi_change >= args.min_oi_change_pct
                     else "oi_contraction" if oi_change is not None and oi_change <= -args.min_oi_change_pct
                     else "ordinary" if oi_change is not None else "baseline_missing")
            observation = {
                "open_interest": oi,
                "oi_change_pct": oi_change,
                "price": quote,
                "price_change_pct": price_change,
                "state": state,
                "research_only": True,
            }
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": {"symbol": args.symbol, "exchange": args.exchange,
                               "min_oi_change_pct": args.min_oi_change_pct},
                "observation": observation,
                "upstream_errors": payload_errors(oi_payload) + payload_errors(quote_payload),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({
        "strategy": "crypto_oi_impulse_response_recorder",
        "output": str(args.output), "appended_snapshots": args.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
