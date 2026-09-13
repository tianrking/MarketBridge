#!/usr/bin/env python3
"""Record macro-reference context, funding and a BTC price snapshot."""

import argparse
import json
import sys
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_macro_context_monitor import classify_macro, macro_rows, quote_value  # noqa: E402


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def find_price(payload, symbol, exchange, product_type):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()
                or str(instrument.get("product_type", product_type)).lower() != product_type.lower()):
            continue
        value = quote_value(row)
        if value is not None:
            return {"price": value, "ts_ms": (row.get("freshness") or {}).get("ts_source"),
                    "stale": bool((row.get("freshness") or {}).get("stale", False))}
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--vix-risk-threshold", type=float, default=25.0)
    parser.add_argument("--funding-extreme-pct", type=float, default=0.01)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-macro-context.jsonl"))
    args = parser.parse_args()
    if (args.vix_risk_threshold <= 0 or args.funding_extreme_pct < 0
            or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid macro threshold, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"symbol": args.symbol, "exchange": args.exchange,
                  "product_type": args.product_type,
                  "vix_risk_threshold": args.vix_risk_threshold,
                  "funding_extreme_pct": args.funding_extreme_pct}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            macro_payload = fetch(args.base_url, "/v1/market/quotes", {
                "exchanges": "dxy,vix,us10y", "include_stale": "false",
            }, args.timeout)
            funding_payload = fetch(args.base_url, "/v1/market/perpetual-funding", {
                "symbols": args.symbol, "exchanges": args.exchange,
                "active_only": "true", "limit": 100,
            }, args.timeout)
            price_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.symbol, "exchanges": args.exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            macro = macro_rows(macro_payload)
            funding = next((row for row in funding_payload.get("funding", [])
                            if str(row.get("symbol", "")).upper() == args.symbol.upper()), None)
            funding_rate = funding.get("funding_rate_pct") if funding else None
            funding_state = "missing_funding"
            if isinstance(funding_rate, (int, float)):
                funding_state = ("long_crowding_context" if funding_rate >= args.funding_extreme_pct else
                                 "short_crowding_context" if funding_rate <= -args.funding_extreme_pct
                                 else "normal_funding_context")
            observation = {
                "macro": macro,
                "funding": {"row": funding, "state": funding_state},
                "context_state": classify_macro(macro, args.vix_risk_threshold),
                "price": find_price(price_payload, args.symbol, args.exchange, args.product_type),
                "evidence": [
                    "macro_quote_available" if macro else "missing_macro_quotes",
                    "funding_row_available" if funding else "missing_funding_row",
                    "price_snapshot_available" if find_price(price_payload, args.symbol, args.exchange, args.product_type)
                    else "missing_price_snapshot",
                ],
                "upstream_errors": macro_payload.get("errors", [])
                + funding_payload.get("errors", []) + price_payload.get("errors", []),
                "research_only": True,
            }
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000),
                                     "iteration": iteration + 1, "parameters": parameters,
                                     "observation": observation}, ensure_ascii=False,
                                    sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_macro_context_recorder", "output": str(args.output),
                      "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
