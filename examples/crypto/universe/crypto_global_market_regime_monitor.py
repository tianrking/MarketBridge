#!/usr/bin/env python3
"""Observe CoinGecko global crypto market context as a research regime.

The hypothesis is deliberately modest: total-market stress and BTC dominance
may describe defensive, broad-risk or alt-rotation states whose later response
can be tested with a separate recorder/replay.  This monitor never allocates or
executes.
"""

import argparse
import json
import time
from urllib.request import Request, urlopen


def fetch(base_url, timeout):
    request = Request(f"{base_url.rstrip('/')}/v1/external/global-market")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(data, key):
    value = data.get(key)
    return float(value) if isinstance(value, (int, float)) else None


def classify_regime(data, dominance_threshold=55.0, stress_threshold=-3.0,
                    breadth_threshold=3.0):
    dominance = number(data, "btc_dominance_pct")
    market_change = number(data, "market_cap_change_24h_pct")
    if dominance is None or market_change is None:
        return "observe_only_missing_global_context"
    if market_change <= stress_threshold:
        return "global_market_stress"
    if dominance >= dominance_threshold and market_change >= 0:
        return "btc_dominant_risk_on"
    if dominance < dominance_threshold and market_change >= breadth_threshold:
        return "broad_market_risk_on"
    return "mixed_global_market_context"


def observe(payload, dominance_threshold, stress_threshold, breadth_threshold):
    data = payload.get("data") or {}
    return {
        "data": data,
        "regime": classify_regime(
            data, dominance_threshold, stress_threshold, breadth_threshold
        ),
        "thresholds": {
            "btc_dominance_pct": dominance_threshold,
            "stress_market_cap_change_24h_pct": stress_threshold,
            "broad_risk_on_market_cap_change_24h_pct": breadth_threshold,
        },
        "limitations": [
            "CoinGecko dominance is a provider aggregate, not a tradable index or exchange price",
            "one snapshot cannot establish a regime transition or forward return",
            "historical sampling, universe construction and execution costs require a separate replay",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--dominance-threshold", type=float, default=55.0)
    parser.add_argument("--stress-threshold", type=float, default=-3.0)
    parser.add_argument("--breadth-threshold", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.timeout <= 0 or args.stress_threshold >= args.breadth_threshold:
        parser.error("timeout must be positive and stress threshold must be below breadth threshold")
    payload = fetch(args.base_url, args.timeout)
    result = observe(
        payload, args.dominance_threshold, args.stress_threshold,
        args.breadth_threshold,
    )
    print(json.dumps({
        "strategy": "crypto_global_market_regime_monitor",
        "observed_at_ms": int(time.time() * 1000),
        "source": payload.get("source"),
        "coverage": payload.get("coverage"),
        "upstream_error": payload.get("error"),
        **result,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
