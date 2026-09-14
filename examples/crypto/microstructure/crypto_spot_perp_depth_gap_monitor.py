#!/usr/bin/env python3
"""Compare target-size spot and perpetual order-book depth.

The observer tests a narrow execution-risk hypothesis: a perp book may offer
materially more executable depth than the same venue's spot book for the same
symbol and target notional. It reports the gap alongside current basis, but it
does not route orders, infer a hedge, or claim that the gap persists.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_liquidity_stress_monitor import book_metrics


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def find_book(payload, symbol, exchange):
    return next((row for row in payload.get("books", [])
                 if str(row.get("symbol", "")).upper() == symbol.upper()
                 and str(row.get("exchange", "")).lower() == exchange.lower()), None)


def max_impact(metrics):
    if not metrics:
        return None
    values = [metrics.get("buy_impact_bps"), metrics.get("sell_impact_bps")]
    values = [value for value in values if isinstance(value, (int, float))]
    return max(values) if values else None


def classify_gap(spot_metrics, perp_metrics, min_depth_ratio, min_impact_improvement_bps):
    if spot_metrics is None or perp_metrics is None:
        return "observe_only_missing_spot_or_perp_book"
    spot_depth = (spot_metrics.get("bid_depth_notional") or 0.0
                  ) + (spot_metrics.get("ask_depth_notional") or 0.0)
    perp_depth = (perp_metrics.get("bid_depth_notional") or 0.0
                  ) + (perp_metrics.get("ask_depth_notional") or 0.0)
    spot_impact = max_impact(spot_metrics)
    perp_impact = max_impact(perp_metrics)
    if spot_depth <= 0 or perp_depth <= 0 or spot_impact is None or perp_impact is None:
        return "observe_only_missing_target_size_depth"
    depth_ratio = perp_depth / spot_depth
    impact_improvement = spot_impact - perp_impact
    if depth_ratio >= min_depth_ratio and impact_improvement >= min_impact_improvement_bps:
        return "perp_depth_advantage_observation"
    if depth_ratio <= 1.0 / min_depth_ratio and impact_improvement <= -min_impact_improvement_bps:
        return "spot_depth_advantage_observation"
    return "no_material_depth_gap"


def observe(base_url, symbol, exchange, target_notional, top_levels,
            min_depth_ratio, min_impact_improvement_bps, timeout):
    spot_payload = fetch(base_url, "/v1/market/order-books", {
        "market": "spot", "symbols": symbol, "exchanges": exchange,
    }, timeout)
    perp_payload = fetch(base_url, "/v1/market/order-books", {
        "market": "perp", "symbols": symbol, "exchanges": exchange,
    }, timeout)
    basis_payload = fetch(base_url, "/v1/market/basis", {
        "symbols": symbol, "exchanges": exchange,
    }, timeout)
    spot_book = find_book(spot_payload, symbol, exchange)
    perp_book = find_book(perp_payload, symbol, exchange)
    spot_metrics = book_metrics(spot_book, target_notional, top_levels)
    perp_metrics = book_metrics(perp_book, target_notional, top_levels)
    state = classify_gap(spot_metrics, perp_metrics, min_depth_ratio,
                         min_impact_improvement_bps)
    basis = next((row for row in basis_payload.get("basis", [])
                  if str(row.get("symbol", "")).upper() == symbol.upper()
                  and str(row.get("exchange", "")).lower() == exchange.lower()), None)
    evidence = [
        "spot_book_available" if spot_book else "missing_spot_book",
        "perp_book_available" if perp_book else "missing_perp_book",
        "basis_context_available" if basis else "missing_basis_context",
    ]
    if spot_metrics and perp_metrics:
        spot_depth = (spot_metrics.get("bid_depth_notional") or 0.0
                      ) + (spot_metrics.get("ask_depth_notional") or 0.0)
        perp_depth = (perp_metrics.get("bid_depth_notional") or 0.0
                      ) + (perp_metrics.get("ask_depth_notional") or 0.0)
        if spot_depth > 0:
            evidence.append(f"perp_to_spot_depth_ratio={perp_depth / spot_depth:.2f}")
        spot_impact = max_impact(spot_metrics)
        perp_impact = max_impact(perp_metrics)
        if spot_impact is not None and perp_impact is not None:
            evidence.append(f"target_size_impact_improvement={spot_impact - perp_impact:.2f} bps")
    return {
        "symbol": symbol,
        "exchange": exchange,
        "state": state,
        "basis_bps": basis.get("basis_bps") if basis else None,
        "spot": {"ts_ms": spot_book.get("ts_ms") if spot_book else None,
                 "metrics": spot_metrics},
        "perp": {"ts_ms": perp_book.get("ts_ms") if perp_book else None,
                 "metrics": perp_metrics},
        "thresholds": {
            "target_notional": target_notional,
            "top_levels": top_levels,
            "min_depth_ratio": min_depth_ratio,
            "min_impact_improvement_bps": min_impact_improvement_bps,
        },
        "evidence": evidence,
        "upstream_errors": (
            spot_payload.get("errors", []) + perp_payload.get("errors", [])
            + basis_payload.get("errors", [])
        ),
        "limitations": [
            "spot and perp snapshots are not synchronized fills",
            "depth ratio is not a routing instruction or hedge feasibility proof",
            "no fees, latency, queue position, basis convergence or position-sizing model",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--target-notional", type=float, default=10_000.0)
    parser.add_argument("--top-levels", type=int, default=10)
    parser.add_argument("--min-depth-ratio", type=float, default=2.0)
    parser.add_argument("--min-impact-improvement-bps", type=float, default=5.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.target_notional <= 0 or args.top_levels <= 0 or args.min_depth_ratio < 1.0
            or args.min_impact_improvement_bps < 0 or args.iterations <= 0
            or args.interval_secs < 0):
        parser.error("invalid target, depth, threshold, iteration or interval arguments")
    for iteration in range(args.iterations):
        result = observe(args.base_url, args.symbol, args.exchange, args.target_notional,
                         args.top_levels, args.min_depth_ratio,
                         args.min_impact_improvement_bps, args.timeout)
        print(json.dumps({"strategy": "crypto_spot_perp_depth_gap_monitor",
                          "iteration": iteration + 1, **result},
                         ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
