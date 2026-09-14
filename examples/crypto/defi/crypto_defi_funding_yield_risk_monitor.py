#!/usr/bin/env python3
"""Observe reward-dependent DeFi yield alongside perp funding context.

The case is intentionally narrow: a reward-heavy pool deserves extra review
when the selected perpetual funding basket is negative. The two snapshots are
not protocol accounting and do not prove that funding caused the displayed
APY. No deposit, withdrawal, wallet or execution path is included.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_defi_yield_context_monitor import classify_pool, number


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def annualized_funding_pct(row):
    rate = number(row.get("funding_rate"))
    interval_ms = number(row.get("funding_interval_ms"))
    if rate is None or interval_ms is None or interval_ms <= 0:
        return None
    return rate * (365.0 * 24.0 * 60.0 * 60.0 * 1000.0 / interval_ms) * 100.0


def summarize_funding(payload, symbols=None, negative_threshold_pct=0.0):
    wanted = {item.strip().upper() for item in (symbols or []) if item.strip()}
    rows = []
    for row in payload.get("funding", []) if isinstance(payload, dict) else []:
        if wanted and str(row.get("symbol", "")).upper() not in wanted:
            continue
        annualized = annualized_funding_pct(row)
        rows.append({
            "exchange": row.get("exchange"),
            "symbol": row.get("symbol"),
            "funding_rate": row.get("funding_rate"),
            "funding_interval_ms": row.get("funding_interval_ms"),
            "annualized_funding_pct": annualized,
        })
    values = [row["annualized_funding_pct"] for row in rows
              if row["annualized_funding_pct"] is not None]
    mean = statistics.mean(values) if values else None
    if mean is None:
        state = "observe_only_missing_funding_context"
    elif mean < -abs(negative_threshold_pct):
        state = "negative_funding_pressure"
    elif mean > abs(negative_threshold_pct):
        state = "positive_funding_carry"
    else:
        state = "near_neutral_funding"
    return {
        "state": state,
        "row_count": len(rows),
        "mean_annualized_funding_pct": mean,
        "median_annualized_funding_pct": statistics.median(values) if values else None,
        "rows": rows,
    }


def summarize(yield_data, funding_payload, funding_symbols=None,
              reward_share_threshold=0.5, negative_threshold_pct=0.0):
    funding = summarize_funding(funding_payload, funding_symbols, negative_threshold_pct)
    pools = yield_data.get("pools", []) if isinstance(yield_data, dict) else []
    pool_rows = []
    state_counts = {}
    for pool in pools:
        yield_state = classify_pool(pool, reward_share_threshold)
        if yield_state == "reward_dependent_yield" and funding["state"] == "negative_funding_pressure":
            state = "funding_sensitive_yield_risk"
        elif yield_state == "reward_dependent_yield" and funding["state"] == "positive_funding_carry":
            state = "funding_supported_reward_context"
        else:
            state = yield_state
        state_counts[state] = state_counts.get(state, 0) + 1
        pool_rows.append({
            "pool_id": pool.get("pool_id"),
            "chain": pool.get("chain"),
            "project": pool.get("project"),
            "symbol": pool.get("symbol"),
            "tvl_usd": number(pool.get("tvl_usd")),
            "apy_pct": number(pool.get("apy_pct")),
            "apy_base_pct": number(pool.get("apy_base_pct")),
            "apy_reward_pct": number(pool.get("apy_reward_pct")),
            "yield_state": yield_state,
            "combined_state": state,
        })
    return {
        "funding": funding,
        "pool_count": len(pool_rows),
        "state_counts": state_counts,
        "pools": pool_rows,
        "research_interpretation": (
            "negative funding is a review flag for reward-dependent yield, not a loss forecast"
            if funding["state"] == "negative_funding_pressure"
            else "funding and yield snapshots are descriptive context"
        ),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--chains", default=None)
    parser.add_argument("--projects", default="ethena-usde,aave-v3,morpho-blue,pendle-v2,curve-dex,convex-finance")
    parser.add_argument("--symbols", default="USDE,SUSDE")
    parser.add_argument("--funding-symbols", default="BTCUSDT,ETHUSDT,SOLUSDT")
    parser.add_argument("--funding-exchange", default=None)
    parser.add_argument("--stablecoin-only", action="store_true")
    parser.add_argument("--min-tvl-usd", type=float, default=None)
    parser.add_argument("--min-apy", type=float, default=None)
    parser.add_argument("--max-apy", type=float, default=None)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--reward-share-threshold", type=float, default=0.5)
    parser.add_argument("--negative-funding-threshold-pct", type=float, default=0.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.limit <= 0 or args.limit > 500 or args.reward_share_threshold < 0
            or args.negative_funding_threshold_pct < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid limits, thresholds, iterations, interval or timeout")
    if args.min_apy is not None and args.max_apy is not None and args.min_apy > args.max_apy:
        parser.error("min-apy cannot exceed max-apy")
    funding_symbols = [item.strip().upper() for item in args.funding_symbols.split(",") if item.strip()]
    if not funding_symbols:
        parser.error("funding-symbols must not be empty")
    for iteration in range(args.iterations):
        yield_payload = fetch(args.base_url, "/v1/external/defi-yields", {
            "chains": args.chains, "projects": args.projects, "symbols": args.symbols,
            "stablecoin": "true" if args.stablecoin_only else None,
            "min_tvl_usd": args.min_tvl_usd, "min_apy": args.min_apy,
            "max_apy": args.max_apy, "limit": args.limit,
        }, args.timeout)
        funding_payload = fetch(args.base_url, "/v1/market/perpetual-funding", {
            "exchange": args.funding_exchange, "symbols": ",".join(funding_symbols),
            "active_only": "true", "limit": 100,
        }, args.timeout)
        data = yield_payload.get("data") or {}
        print(json.dumps({
            "strategy": "crypto_defi_funding_yield_risk_monitor",
            "iteration": iteration + 1,
            "filters": {
                "chains": args.chains, "projects": args.projects, "symbols": args.symbols,
                "funding_symbols": funding_symbols, "funding_exchange": args.funding_exchange,
                "stablecoin": args.stablecoin_only, "min_tvl_usd": args.min_tvl_usd,
                "min_apy": args.min_apy, "max_apy": args.max_apy,
                "limit": args.limit, "reward_share_threshold": args.reward_share_threshold,
                "negative_funding_threshold_pct": args.negative_funding_threshold_pct,
            },
            "summary": summarize(data, funding_payload, funding_symbols,
                                 args.reward_share_threshold, args.negative_funding_threshold_pct),
            "provider_summary": data.get("summary", {}),
            "limitations": [
                "DefiLlama APY is not protocol accounting and may lag the funding snapshot",
                "the selected funding basket is a transparent proxy, not Ethena's private hedge book",
                "negative funding is a review condition, not a liquidation, depeg or loss forecast",
            ],
            "upstream_errors": yield_payload.get("errors", []) + funding_payload.get("errors", []),
            "execution": "research_only_no_orders",
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
