#!/usr/bin/env python3
"""Classify Bitcoin mining stress as read-only network context.

The falsifiable hypothesis is non-directional: a materially negative difficulty
adjustment or seven-day hashrate change may be followed by a different BTC
absolute-move distribution than ordinary mining-context snapshots. It does not
identify miner capitulation, forecast BTC, or model miner cash flow.
"""

import argparse
import json
from urllib.request import Request, urlopen


def fetch(base_url, timeout):
    request = Request(f"{base_url.rstrip('/')}/v1/onchain/mining")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def classify(data, stress_difficulty_pct, stress_hashrate_pct, tailwind_difficulty_pct,
             tailwind_hashrate_pct):
    if not isinstance(data, dict):
        return "observe_only_missing_mining_metrics"
    difficulty = number(data.get("difficulty_change_pct"))
    hashrate = number(data.get("hashrate_change_7d_pct"))
    if difficulty is None or hashrate is None:
        return "observe_only_missing_mining_metrics"
    if difficulty <= stress_difficulty_pct or hashrate <= stress_hashrate_pct:
        return "miner_stress_context"
    if difficulty >= tailwind_difficulty_pct and hashrate >= tailwind_hashrate_pct:
        return "miner_tailwind_context"
    return "ordinary_mining_context"


def summarize(data, stress_difficulty_pct, stress_hashrate_pct, tailwind_difficulty_pct,
              tailwind_hashrate_pct):
    return {
        "state": classify(data, stress_difficulty_pct, stress_hashrate_pct,
                           tailwind_difficulty_pct, tailwind_hashrate_pct),
        "difficulty_change_pct": number(data.get("difficulty_change_pct")) if isinstance(data, dict) else None,
        "hashrate_change_7d_pct": number(data.get("hashrate_change_7d_pct")) if isinstance(data, dict) else None,
        "current_hashrate_hs": number(data.get("current_hashrate_hs")) if isinstance(data, dict) else None,
        "current_difficulty": number(data.get("current_difficulty")) if isinstance(data, dict) else None,
        "difficulty_progress_pct": number(data.get("difficulty_progress_pct")) if isinstance(data, dict) else None,
        "remaining_blocks": data.get("remaining_blocks") if isinstance(data, dict) else None,
        "hashrate_samples": data.get("hashrate_samples") if isinstance(data, dict) else None,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--stress-difficulty-pct", type=float, default=-3.0)
    parser.add_argument("--stress-hashrate-pct", type=float, default=-3.0)
    parser.add_argument("--tailwind-difficulty-pct", type=float, default=3.0)
    parser.add_argument("--tailwind-hashrate-pct", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.stress_difficulty_pct > args.tailwind_difficulty_pct
            or args.stress_hashrate_pct > args.tailwind_hashrate_pct
            or args.timeout <= 0):
        parser.error("stress thresholds must be <= tailwind thresholds")
    payload = fetch(args.base_url, args.timeout)
    data = payload.get("data") or {}
    print(json.dumps({
        "strategy": "crypto_onchain_mining_pressure_monitor",
        "summary": summarize(data, args.stress_difficulty_pct, args.stress_hashrate_pct,
                              args.tailwind_difficulty_pct, args.tailwind_hashrate_pct),
        "thresholds": {
            "stress_difficulty_pct": args.stress_difficulty_pct,
            "stress_hashrate_pct": args.stress_hashrate_pct,
            "tailwind_difficulty_pct": args.tailwind_difficulty_pct,
            "tailwind_hashrate_pct": args.tailwind_hashrate_pct,
        },
        "limitations": [
            "hashrate and difficulty are provider estimates, not miner identity or profitability",
            "stress labels do not prove capitulation, forced selling or price causality",
            "this is network context, not a BTC direction forecast or execution instruction",
        ],
        "upstream_errors": ([payload["error"]] if payload.get("error") else []),
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
