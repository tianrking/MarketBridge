#!/usr/bin/env python3
"""Research whether current funding is close to an exchange provider band.

The provider band is context, not a directional signal: this monitor reports
when a current funding observation is close to Binance's published adjusted
cap/floor and leaves the next-return or convergence test to a replay.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/market/perpetual-funding?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(row, key):
    value = row.get(key)
    return float(value) if isinstance(value, (int, float)) else None


def classify_band(row, threshold):
    rate = number(row, "funding_rate")
    cap = number(row, "funding_rate_cap")
    floor = number(row, "funding_rate_floor")
    if rate is None or cap is None or floor is None or cap <= 0 or floor >= 0:
        return {
            "state": "observe_only_missing_provider_band",
            "proximity": None,
        }
    upper = rate / cap if rate >= 0 else None
    lower = rate / floor if rate < 0 else None
    proximity = upper if upper is not None else lower
    if upper is not None and upper >= threshold:
        state = "near_upper_funding_cap"
    elif lower is not None and lower >= threshold:
        state = "near_lower_funding_floor"
    else:
        state = "within_provider_funding_band"
    return {
        "state": state,
        "proximity": proximity,
        "distance_to_cap_or_floor": 1.0 - proximity,
    }


def observe(payload, symbol, threshold):
    rows = []
    for row in payload.get("funding", []):
        if symbol and str(row.get("symbol", "")).upper() != symbol.upper():
            continue
        band = classify_band(row, threshold)
        rows.append({
            "exchange": row.get("exchange"),
            "symbol": row.get("symbol"),
            "funding_rate": row.get("funding_rate"),
            "funding_interval_ms": row.get("funding_interval_ms"),
            "funding_rate_cap": row.get("funding_rate_cap"),
            "funding_rate_floor": row.get("funding_rate_floor"),
            **band,
        })
    candidates = [row for row in rows if row["state"] in {
        "near_upper_funding_cap", "near_lower_funding_floor"
    }]
    return {
        "rows": rows,
        "band_candidates": candidates,
        "evidence": [
            "provider_band_proximity_is_observable" if candidates
            else "no_row_reached_provider_band_threshold",
        ],
        "limitations": [
            "adjusted cap and floor are provider parameters and can change",
            "proximity does not forecast price, liquidation or realized funding",
            "historical next-window returns and all execution costs require a separate replay",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--threshold", type=float, default=0.8)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if not 0 < args.threshold <= 1:
        parser.error("threshold must be between 0 and 1")
    if args.timeout <= 0:
        parser.error("timeout must be positive")
    payload = fetch(args.base_url, {
        "exchange": args.exchange,
        "symbols": args.symbol,
        "active_only": "true",
        "limit": 100,
    }, args.timeout)
    print(json.dumps({
        "strategy": "crypto_funding_band_monitor",
        "symbol": args.symbol,
        "exchange": args.exchange,
        "observed_at_ms": int(time.time() * 1000),
        "threshold": args.threshold,
        "supported_exchanges": payload.get("supported_exchanges", []),
        "upstream_errors": payload.get("errors", []),
        **observe(payload, args.symbol, args.threshold),
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
