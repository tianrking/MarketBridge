#!/usr/bin/env python3
"""Observe DeFi pool yield composition as read-only research context.

This monitor separates provider-reported base APY from reward APY. It does not
assume that a quoted APY is stable, liquid, safe, or executable.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}/v1/external/defi-yields{suffix}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def classify_pool(pool, reward_share_threshold):
    apy = number(pool.get("apy_pct"))
    base = number(pool.get("apy_base_pct"))
    reward = number(pool.get("apy_reward_pct"))
    if apy is None or number(pool.get("tvl_usd")) is None:
        return "observe_only_missing_yield_metrics"
    if apy <= 0:
        return "non_positive_apy"
    share = max(reward or 0.0, 0.0) / apy
    if share >= reward_share_threshold:
        return "reward_dependent_yield"
    if base is not None and base > 0:
        return "base_yield_dominant"
    return "observe_only_unclassified_yield"


def summarize(data, reward_share_threshold=0.5):
    pools = data.get("pools", []) if isinstance(data, dict) else []
    counts = {}
    classified = []
    for pool in pools:
        state = classify_pool(pool, reward_share_threshold)
        counts[state] = counts.get(state, 0) + 1
        classified.append({
            "pool_id": pool.get("pool_id"),
            "chain": pool.get("chain"),
            "project": pool.get("project"),
            "symbol": pool.get("symbol"),
            "tvl_usd": number(pool.get("tvl_usd")),
            "apy_pct": number(pool.get("apy_pct")),
            "apy_base_pct": number(pool.get("apy_base_pct")),
            "apy_reward_pct": number(pool.get("apy_reward_pct")),
            "state": state,
        })
    return {
        "pool_count": len(pools),
        "state_counts": counts,
        "reward_share_threshold": reward_share_threshold,
        "pools": classified,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--chains", default=None)
    parser.add_argument("--projects", default=None)
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--stablecoin-only", action="store_true")
    parser.add_argument("--min-tvl-usd", type=float, default=None)
    parser.add_argument("--min-apy", type=float, default=None)
    parser.add_argument("--max-apy", type=float, default=None)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--reward-share-threshold", type=float, default=0.5)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.limit <= 0 or args.limit > 500 or args.reward_share_threshold < 0
            or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("limit, reward threshold, iterations, interval and timeout must be valid")
    if args.min_apy is not None and args.max_apy is not None and args.min_apy > args.max_apy:
        parser.error("min-apy cannot exceed max-apy")
    for iteration in range(args.iterations):
        payload = fetch(args.base_url, {
            "chains": args.chains,
            "projects": args.projects,
            "symbols": args.symbols,
            "stablecoin": "true" if args.stablecoin_only else None,
            "min_tvl_usd": args.min_tvl_usd,
            "min_apy": args.min_apy,
            "max_apy": args.max_apy,
            "limit": args.limit,
        }, args.timeout)
        data = payload.get("data") or {}
        print(json.dumps({
            "strategy": "crypto_defi_yield_context_monitor",
            "iteration": iteration + 1,
            "filters": {
                "chains": args.chains,
                "projects": args.projects,
                "symbols": args.symbols,
                "stablecoin": args.stablecoin_only,
                "min_tvl_usd": args.min_tvl_usd,
                "min_apy": args.min_apy,
                "max_apy": args.max_apy,
                "limit": args.limit,
            },
            "summary": summarize(data, args.reward_share_threshold),
            "provider_summary": data.get("summary", {}),
            "limitations": [
                "APY and TVL are provider snapshots, not guaranteed future yield or liquidity",
                "reward APY depends on reward-token prices and emissions",
                "pool safety, audits, lockups, impermanent loss and contract risk are not certified",
            ],
            "execution": "research_only_no_orders",
            "errors": payload.get("errors", []) + ([payload["error"]] if payload.get("error") else []),
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
