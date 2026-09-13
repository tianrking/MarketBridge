#!/usr/bin/env python3
"""Monitor missing/stale quote risk before using a crypto universe.

This is a data-quality guard, not a delisting forecast.  It asks whether a
historically observed market still has a current quote in MarketBridge and
keeps high/medium-risk rows out of downstream research unless a caller reviews
them explicitly.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/universe/delist-risk?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def summarize(rows):
    rows = [row for row in rows if isinstance(row, dict)]
    high = [row for row in rows if str(row.get("risk", "")).lower() == "high"]
    medium = [row for row in rows if str(row.get("risk", "")).lower() == "medium"]
    return {
        "rows": len(rows),
        "high_risk_rows": len(high),
        "medium_risk_rows": len(medium),
        "low_risk_rows": sum(str(row.get("risk", "")).lower() == "low" for row in rows),
        "reasons": {
            reason: sum(row.get("reason") == reason for row in rows)
            for reason in sorted({row.get("reason") for row in rows if row.get("reason")})
        },
        "verdict": "data_quality_review_required" if high or medium else "no_stale_or_missing_quote_rows",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default=None)
    parser.add_argument("--market", choices=("spot", "perp"), default=None)
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--stale-after-ms", type=int, default=86_400_000)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.stale_after_ms <= 0 or not 1 <= args.limit <= 1000 or args.timeout <= 0:
        parser.error("stale-after-ms, limit and timeout must be positive")
    payload = fetch(args.base_url, {
        "exchange": args.exchange, "market": args.market, "symbols": args.symbols,
        "interval": args.interval, "stale_after_ms": args.stale_after_ms, "limit": args.limit,
    }, args.timeout)
    rows = payload.get("rows", [])
    print(json.dumps({
        "strategy": "crypto_universe_delist_risk_monitor",
        "filters": {
            "exchange": args.exchange, "market": args.market, "symbols": args.symbols,
            "interval": args.interval, "stale_after_ms": args.stale_after_ms,
            "limit": args.limit,
        },
        "summary": summarize(rows),
        "rows": rows,
        "limitations": [
            "missing or stale quotes are a data-quality risk, not proof of delisting",
            "historical kline retention and quote freshness are provider/configuration dependent",
            "no order, routing, wallet or automatic exclusion side effect is performed",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
