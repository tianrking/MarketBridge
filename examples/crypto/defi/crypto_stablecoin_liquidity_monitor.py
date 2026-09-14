#!/usr/bin/env python3
"""Observe stablecoin supply expansion/contraction as liquidity context.

The falsifiable hypothesis is narrow: broad stablecoin supply growth may
coincide with a different subsequent crypto response than supply contraction.
This monitor only reports DefiLlama's provider snapshot and does not equate
circulating supply with exchange balances, deployable liquidity or a price
forecast.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/external/stablecoins?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def summarize(data, growth_threshold_pct):
    assets = data.get("assets", []) if isinstance(data, dict) else []
    changes = [number(row.get("change_7d_pct")) for row in assets]
    changes = [value for value in changes if value is not None]
    mean_change = statistics.mean(changes) if changes else None
    if mean_change is None:
        state = "observe_only_missing_supply_change"
    elif mean_change >= growth_threshold_pct:
        state = "stablecoin_supply_expansion"
    elif mean_change <= -growth_threshold_pct:
        state = "stablecoin_supply_contraction"
    else:
        state = "stablecoin_supply_flat"
    return {
        "state": state,
        "asset_count": len(assets),
        "mean_asset_change_7d_pct": mean_change,
        "median_asset_change_7d_pct": statistics.median(changes) if changes else None,
        "total_supply_usd": number(data.get("total_supply_usd")) if isinstance(data, dict) else None,
        "chain_count": len(data.get("chains", [])) if isinstance(data, dict) else 0,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default=None, help="optional comma-separated stablecoin symbols")
    parser.add_argument("--peg-type", default="peggedUSD")
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--growth-threshold-pct", type=float, default=1.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.limit <= 0 or args.growth_threshold_pct < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("limit, threshold, iterations, interval and timeout must be valid")
    for iteration in range(args.iterations):
        payload = fetch(args.base_url, {
            "symbols": args.symbols,
            "peg_type": args.peg_type,
            "limit": min(args.limit, 500),
        }, args.timeout)
        data = payload.get("data") or {}
        print(json.dumps({
            "strategy": "crypto_stablecoin_liquidity_monitor",
            "iteration": iteration + 1,
            "filters": {"symbols": args.symbols, "peg_type": args.peg_type,
                        "growth_threshold_pct": args.growth_threshold_pct},
            "summary": summarize(data, args.growth_threshold_pct),
            "assets": data.get("assets", []),
            "errors": payload.get("errors", []) + ([payload["error"]] if payload.get("error") else []),
            "limitations": [
                "circulating supply is not exchange inventory or immediately deployable liquidity",
                "provider snapshots and seven-day changes do not establish causality or price direction",
                "no borrow, peg, bridge, venue solvency, fee or execution model is included",
            ],
            "execution": "research_only_no_orders",
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
