#!/usr/bin/env python3
"""Observe read-only Jupiter route-impact ladders from MarketBridge.

The monitor consumes Jupiter's configured quote-size ladder emitted as
``defi_native_state`` metrics.  It labels high route-price-impact observations
as execution-risk context; it never builds, signs or submits a swap.
"""

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_microstructure_monitor import fetch  # noqa: E402


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def metric_key(metric, prefix):
    marker = f"{prefix}_"
    if not isinstance(metric, str) or not metric.startswith(marker):
        return None
    suffix = metric[len(marker):]
    try:
        amount = int(suffix)
    except ValueError:
        return None
    return amount if amount > 0 else None


def summarize_route_signals(signals, min_impact_ratio):
    grouped = {}
    for row in signals:
        if not isinstance(row, dict):
            continue
        source = str(row.get("source", "")).lower()
        symbol = str(row.get("symbol", "")).upper()
        metric = row.get("metric")
        value = number(row.get("value"))
        amount = next((metric_key(metric, prefix) for prefix in (
            "route_price_impact_ratio", "route_hops", "route_price",
            "route_output_amount_atomic", "route_input_amount_atomic",
        ) if metric_key(metric, prefix) is not None), None)
        if not source or not symbol or amount is None or value is None or not math.isfinite(value):
            continue
        key = (source, symbol, amount)
        grouped.setdefault(key, {})
        grouped[key]["freshness_ts_ms"] = (row.get("freshness") or {}).get("ts_source")
        impact = metric_key(metric, "route_price_impact_ratio")
        if impact == amount:
            grouped[key]["price_impact_ratio"] = value
        hops = metric_key(metric, "route_hops")
        if hops == amount:
            grouped[key]["hops"] = value
        price = metric_key(metric, "route_price")
        if price == amount:
            grouped[key]["route_price"] = value
        output = metric_key(metric, "route_output_amount_atomic")
        if output == amount:
            grouped[key]["output_amount_atomic"] = value
    routes = []
    for (source, symbol, amount), metrics in sorted(grouped.items()):
        impact = metrics.get("price_impact_ratio")
        routes.append({
            "source": source,
            "symbol": symbol,
            "input_amount_atomic": amount,
            "price_impact_ratio": impact,
            "price_impact_pct": impact * 100.0 if impact is not None else None,
            "hops": metrics.get("hops"),
            "route_price": metrics.get("route_price"),
            "output_amount_atomic": metrics.get("output_amount_atomic"),
            "freshness_ts_ms": metrics.get("freshness_ts_ms"),
            "state": "observe_only_missing_route" if impact is None else (
                "high_route_impact" if impact >= min_impact_ratio else "normal_route_impact"
            ),
        })
    return routes


def observe_routes(base_url, symbols, min_impact_ratio, timeout):
    payload = fetch(base_url, "/v1/external/signals", {
        "categories": "defi_native_state",
        "sources": "jupiter",
        "symbols": symbols or None,
    }, timeout)
    routes = summarize_route_signals(payload.get("signals", []), min_impact_ratio)
    return {
        "routes": routes,
        "route_count": len(routes),
        "upstream_errors": payload.get("errors", []),
        "evidence": ["jupiter_route_ladder_available" if routes else "missing_jupiter_route_ladder"],
        "limitations": [
            "route metrics are public quote snapshots, not guaranteed executable depth",
            "configured input-size ladders are caller-selected and may be sparse",
            "price impact is a router-reported ratio and does not model gas, latency, MEV or fills",
            "no transaction, wallet, signing or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--min-impact-ratio", type=float, default=0.005)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if not 0 <= args.min_impact_ratio <= 1 or args.timeout <= 0:
        parser.error("impact ratio must be in [0, 1] and timeout must be positive")
    print(json.dumps({
        "strategy": "crypto_jupiter_route_impact_monitor",
        "filters": {"symbols": args.symbols, "min_impact_ratio": args.min_impact_ratio},
        **observe_routes(args.base_url, args.symbols, args.min_impact_ratio, args.timeout),
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
