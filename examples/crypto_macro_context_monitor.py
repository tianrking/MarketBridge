#!/usr/bin/env python3
"""Observe macro-reference context alongside crypto funding.

This read-only monitor joins configured DXY, VIX and US10Y quote snapshots
with current perp funding context. It labels a research backdrop only; it does
not claim a causal crypto return forecast or choose a trade.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def quote_value(row):
    payload = row.get("payload") or {}
    for key in ("mark", "mid", "price", "last"):
        value = payload.get(key)
        if isinstance(value, (int, float)) and value > 0:
            return float(value)
    bid, ask = payload.get("bid"), payload.get("ask")
    if isinstance(bid, (int, float)) and isinstance(ask, (int, float)) and bid > 0 and ask > 0:
        return (bid + ask) / 2.0
    return None


def macro_rows(payload):
    rows = {}
    for row in payload.get("quotes", []):
        source = str((row.get("source_ref") or {}).get("source", "")).lower()
        if source in {"dxy", "vix", "us10y"}:
            rows[source] = {
                "value": quote_value(row),
                "stale": bool((row.get("freshness") or {}).get("stale", False)),
                "ts_ms": (row.get("freshness") or {}).get("ts_source"),
            }
    return rows


def classify_macro(rows, vix_risk_threshold):
    vix = (rows.get("vix") or {}).get("value")
    if vix is None:
        return "observe_only_missing_vix"
    if vix >= vix_risk_threshold:
        return "elevated_volatility_context"
    return "normal_volatility_context"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--vix-risk-threshold", type=float, default=25.0)
    parser.add_argument("--funding-extreme-pct", type=float, default=0.01)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.vix_risk_threshold <= 0 or args.funding_extreme_pct < 0 or args.timeout <= 0:
        parser.error("VIX threshold, funding threshold and timeout must be valid")
    macro_payload = fetch(args.base_url, "/v1/market/quotes", {
        "exchanges": "dxy,vix,us10y", "include_stale": "false",
    }, args.timeout)
    funding_payload = fetch(args.base_url, "/v1/market/perpetual-funding", {
        "symbols": args.symbol, "exchanges": args.exchange,
        "active_only": "true", "limit": 100,
    }, args.timeout)
    rows = macro_rows(macro_payload)
    funding = next((row for row in funding_payload.get("funding", [])
                    if str(row.get("symbol", "")).upper() == args.symbol.upper()), None)
    funding_rate = funding.get("funding_rate_pct") if funding else None
    funding_state = "missing_funding"
    if isinstance(funding_rate, (int, float)):
        funding_state = "long_crowding_context" if funding_rate >= args.funding_extreme_pct else (
            "short_crowding_context" if funding_rate <= -args.funding_extreme_pct else "normal_funding_context"
        )
    print(json.dumps({
        "strategy": "crypto_macro_context_monitor",
        "market": {"symbol": args.symbol, "exchange": args.exchange},
        "macro": rows,
        "funding": {"row": funding, "state": funding_state},
        "context_state": classify_macro(rows, args.vix_risk_threshold),
        "evidence": [
            "macro_quote_available" if rows else "missing_macro_quotes",
            "funding_row_available" if funding else "missing_funding_row",
        ],
        "upstream_errors": macro_payload.get("errors", []) + funding_payload.get("errors", []),
        "limitations": [
            "DXY, VIX and US10Y are current reference snapshots, not synchronized historical factors",
            "VIX measures SPX option-implied volatility and is not crypto-implied volatility",
            "funding is a crowding context, not a position ledger or trade signal",
            "no macro forecast, allocation, order, wallet or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
