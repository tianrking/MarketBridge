#!/usr/bin/env python3
"""Observe Binance symbol-level ADL risk without treating it as a trade signal."""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def classify_risk(risk):
    value = str(risk or "").strip().lower()
    if value == "high":
        return "adl_risk_high"
    if value == "medium":
        return "adl_risk_medium"
    if value == "low":
        return "adl_risk_low"
    return "observe_only_unknown_adl_risk"


def summarize(rows):
    by_state = {}
    for row in rows:
        state = classify_risk(row.get("adl_risk"))
        by_state[state] = by_state.get(state, 0) + 1
    return {
        "rows": len(rows),
        "by_state": by_state,
        "high_risk_symbols": [row.get("symbol") for row in rows
                              if classify_risk(row.get("adl_risk")) == "adl_risk_high"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("timeout must be positive")
    payload = fetch(args.base_url, "/v1/market/adl-risk", {
        "exchange": args.exchange, "symbol": args.symbol,
    }, args.timeout)
    rows = payload.get("rows", [])
    print(json.dumps({
        "strategy": "crypto_adl_risk_monitor",
        "exchange": args.exchange,
        "symbol": args.symbol,
        "observed_at_ms": int(time.time() * 1000),
        "coverage": payload.get("coverage"),
        "updated_every_minutes": payload.get("updated_every_minutes"),
        "rows": rows,
        "summary": summarize(rows),
        "upstream_errors": ([{"source": "adl_risk", "error": payload["error"]}]
                            if payload.get("error") else []),
        "limitations": [
            "ADL risk is a provider rating and not a directional price forecast",
            "the public snapshot does not expose private account risk or guarantee an ADL event",
            "no order, wallet, signing or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
