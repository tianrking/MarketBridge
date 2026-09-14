#!/usr/bin/env python3
"""Replay historical Binance basis contraction.

The falsifiable carry hypothesis is deliberately narrow: after an unusually
wide provider basis-rate observation, does the absolute basis rate contract by
the next fixed provider window more often than ordinary observations? This is
not simultaneous bid/ask arbitrage, hedge PnL or an execution model.
"""

import argparse
import bisect
import json
import statistics
import time

from crypto_funding_spread_response_replay import fetch


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def basis_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        basis_rate_bps = number(row.get("basis_rate_bps"))
        basis = number(row.get("basis"))
        if isinstance(timestamp, int) and basis_rate_bps is not None and basis is not None:
            points.append((timestamp, basis_rate_bps, basis))
    return sorted(set(points))


def forward_basis(points, timestamp, horizon_bars):
    if horizon_bars <= 0 or not points:
        return None
    timestamps = [point[0] for point in points]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if index >= len(points) or future_index >= len(points):
        return None
    current = points[index]
    future = points[future_index]
    return {
        "forward_ts_ms": future[0],
        "forward_basis_rate_bps": future[1],
        "basis_rate_contraction_bps": abs(current[1]) - abs(future[1]),
        "basis_contraction": abs(current[2]) - abs(future[2]),
    }


def classify_state(basis_rate_bps, extreme_threshold_bps):
    if basis_rate_bps is None:
        return "observe_only_missing_basis"
    return "extreme_basis" if abs(basis_rate_bps) >= extreme_threshold_bps else "ordinary_basis"


def observations(points, horizon_bars, extreme_threshold_bps):
    rows = []
    for timestamp, basis_rate_bps, basis in points:
        future = forward_basis(points, timestamp, horizon_bars)
        rows.append({
            "ts_ms": timestamp,
            "basis_rate_bps": basis_rate_bps,
            "basis": basis,
            "state": classify_state(basis_rate_bps, extreme_threshold_bps),
            "forward_ts_ms": future["forward_ts_ms"] if future else None,
            "forward_basis_rate_bps": future["forward_basis_rate_bps"] if future else None,
            "basis_rate_contraction_bps": future["basis_rate_contraction_bps"] if future else None,
            "basis_contraction": future["basis_contraction"] if future else None,
        })
    return rows


def stats(rows):
    contraction = [row["basis_rate_contraction_bps"] for row in rows
                   if row["basis_rate_contraction_bps"] is not None]
    return {
        "observations": len(rows),
        "forward_observations": len(contraction),
        "mean_basis_rate_contraction_bps": statistics.mean(contraction) if contraction else None,
        "contraction_frequency": (sum(value > 0 for value in contraction) / len(contraction)
                                  if contraction else None),
    }


def summarize(rows, min_observations, min_contraction_bps):
    extreme = stats([row for row in rows if row["state"] == "extreme_basis"])
    ordinary = stats([row for row in rows if row["state"] == "ordinary_basis"])
    edge = (extreme["mean_basis_rate_contraction_bps"] - ordinary["mean_basis_rate_contraction_bps"]
            if extreme["mean_basis_rate_contraction_bps"] is not None
            and ordinary["mean_basis_rate_contraction_bps"] is not None else None)
    return {
        "extreme_basis": extreme,
        "ordinary_basis": ordinary,
        "contraction_edge_bps": edge,
        "min_contraction_bps": min_contraction_bps,
        "verdict": "historical_basis_contraction_candidate"
        if (extreme["observations"] >= min_observations and edge is not None and edge >= min_contraction_bps)
        else "observe_only",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--contract-type", default="PERPETUAL")
    parser.add_argument("--period", default="1h")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--basis-pages", type=int, default=1,
                        help="bounded Binance basis-history pages (1-48)")
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--extreme-threshold-bps", type=float, default=50.0)
    parser.add_argument("--min-contraction-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or args.basis_pages < 1 or args.basis_pages > 48
            or args.horizon_bars <= 0
            or args.extreme_threshold_bps < 0 or args.min_contraction_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, threshold, horizon, observation or timeout argument")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/basis", {
        "exchange": args.exchange, "symbol": args.symbol,
        "contract_type": args.contract_type, "period": args.period,
        "start_ms": start_ms, "end_ms": now_ms, "limit": min(args.limit, 500),
        "pages": args.basis_pages,
    }, args.timeout)
    points = basis_points(payload)
    rows = observations(points, args.horizon_bars, args.extreme_threshold_bps)
    print(json.dumps({
        "strategy": "crypto_historical_basis_replay",
        "exchange": args.exchange, "symbol": args.symbol,
        "contract_type": args.contract_type, "period": args.period,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"extreme_threshold_bps": args.extreme_threshold_bps,
                    "basis_pages": args.basis_pages,
                    "horizon_bars": args.horizon_bars,
                    "min_contraction_bps": args.min_contraction_bps,
                    "min_observations": args.min_observations},
        "source_counts": {"basis_points": len(points)},
        "observations": rows,
        "summary": summarize(rows, args.min_observations, args.min_contraction_bps),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": ([{"source": "basis", "error": payload["error"]}]
                            if payload.get("error") else []),
        "limitations": [
            "basis is a provider snapshot, not simultaneous executable bid/ask legs",
            "contraction excludes fees, borrow, funding, transfers, inventory and hedge slippage",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
