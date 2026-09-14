#!/usr/bin/env python3
"""Classify Bitcoin mempool fee pressure as read-only market context.

The falsifiable hypothesis is non-directional: unusually high or low Bitcoin
fee pressure may coincide with a different later BTC absolute-move distribution
than ordinary snapshots. This monitor only labels the current provider snapshot;
it does not forecast price, choose a transaction fee, or broadcast anything.
"""

import argparse
import json
from urllib.request import Request, urlopen


def fetch(base_url, timeout):
    request = Request(f"{base_url.rstrip('/')}/v1/onchain/mempool")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def classify(data, high_fee_sat_vb, low_fee_sat_vb, high_vsize_mb, low_vsize_mb):
    if not isinstance(data, dict):
        return "observe_only_missing_mempool_metrics"
    fees = data.get("fee_rates_sat_vb") or {}
    fastest = number(fees.get("fastest"))
    vsize_mb = number(data.get("mempool_vsize_mb"))
    if fastest is None or vsize_mb is None:
        return "observe_only_missing_mempool_metrics"
    if fastest >= high_fee_sat_vb or vsize_mb >= high_vsize_mb:
        return "high_fee_pressure"
    if fastest <= low_fee_sat_vb and vsize_mb <= low_vsize_mb:
        return "low_fee_pressure"
    return "ordinary_fee_pressure"


def summarize(data, high_fee_sat_vb, low_fee_sat_vb, high_vsize_mb, low_vsize_mb):
    fees = data.get("fee_rates_sat_vb") or {} if isinstance(data, dict) else {}
    return {
        "state": classify(data, high_fee_sat_vb, low_fee_sat_vb, high_vsize_mb, low_vsize_mb),
        "mempool_count": data.get("mempool_count") if isinstance(data, dict) else None,
        "mempool_vsize_mb": number(data.get("mempool_vsize_mb")) if isinstance(data, dict) else None,
        "fastest_fee_sat_vb": number(fees.get("fastest")),
        "half_hour_fee_sat_vb": number(fees.get("half_hour")),
        "hour_fee_sat_vb": number(fees.get("hour")),
        "economy_fee_sat_vb": number(fees.get("economy")),
        "tip_height": data.get("tip_height") if isinstance(data, dict) else None,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--high-fee-sat-vb", type=float, default=20.0)
    parser.add_argument("--low-fee-sat-vb", type=float, default=3.0)
    parser.add_argument("--high-vsize-mb", type=float, default=150.0)
    parser.add_argument("--low-vsize-mb", type=float, default=25.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.high_fee_sat_vb < args.low_fee_sat_vb or args.low_fee_sat_vb < 0
            or args.high_vsize_mb < args.low_vsize_mb or args.low_vsize_mb < 0
            or args.timeout <= 0):
        parser.error("high thresholds must be >= low thresholds and non-negative")
    payload = fetch(args.base_url, args.timeout)
    data = payload.get("data") or {}
    print(json.dumps({
        "strategy": "crypto_onchain_mempool_pressure_monitor",
        "summary": summarize(data, args.high_fee_sat_vb, args.low_fee_sat_vb,
                              args.high_vsize_mb, args.low_vsize_mb),
        "thresholds": {
            "high_fee_sat_vb": args.high_fee_sat_vb,
            "low_fee_sat_vb": args.low_fee_sat_vb,
            "high_vsize_mb": args.high_vsize_mb,
            "low_vsize_mb": args.low_vsize_mb,
        },
        "limitations": [
            "mempool state is provider- and node-dependent, not a complete network-wide ledger",
            "recommended fee rates do not guarantee confirmation timing",
            "fee pressure is context, not a directional BTC forecast or transaction instruction",
        ],
        "upstream_errors": ([payload["error"]] if payload.get("error") else []),
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
