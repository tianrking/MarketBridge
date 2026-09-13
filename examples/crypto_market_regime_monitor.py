#!/usr/bin/env python3
"""Read the MarketBridge aggregate market-regime context for research.

The monitor translates the Rust feature bundle into explicit research
conditions. It does not choose a strategy, size a position, or execute orders.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/research/market-regime?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def research_context(snapshot):
    regime = str(snapshot.get("regime", "unknown"))
    guidance = {
        "fragmented": "require_cross_venue_freshness_and_cost_checks",
        "high_volatility": "use_stress_sensitive_research_thresholds",
        "leveraged": "treat_funding_as_crowding_context",
        "normal": "no_aggregate_regime_warning",
    }.get(regime, "observe_only_unknown_regime")
    return {"regime": regime, "research_guidance": guidance, "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default="BTCUSDT,ETHUSDT")
    parser.add_argument("--exchange", default=None)
    parser.add_argument("--market", default=None)
    parser.add_argument("--intervals", default="1h,4h,1d")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("timeout must be positive")
    payload = fetch(args.base_url, {
        "symbols": args.symbols, "exchange": args.exchange, "market": args.market,
        "intervals": args.intervals,
    }, args.timeout)
    snapshot = payload.get("snapshot") or {}
    print(json.dumps({
        "strategy": "crypto_market_regime_monitor",
        "filters": {"symbols": args.symbols, "exchange": args.exchange,
                    "market": args.market, "intervals": args.intervals},
        "snapshot": snapshot,
        "context": research_context(snapshot),
        "limitations": [
            "the aggregate regime is a current feature snapshot, not historical point-in-time data",
            "research guidance is contextual and does not select, size or execute a strategy",
            "missing feature rows remain evidence gaps rather than neutral values",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
