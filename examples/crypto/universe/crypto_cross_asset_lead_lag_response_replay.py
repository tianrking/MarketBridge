#!/usr/bin/env python3
"""Replay a leader asset's past return against a follower's future return.

The replay uses exact candle timestamp intersections.  A leader move is known
only through the current candle; the follower response starts after that
timestamp and is measured over a fixed future horizon.  This tests an
observable lead-lag association, not Granger causality, a portfolio, or an
execution rule.
"""

import argparse
import json
import math
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def candle_points(payload):
    points = {}
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points[timestamp] = close
    return points


def classify_leader(leader_return_pct, threshold_pct):
    if leader_return_pct is None:
        return "observe_only_missing_leader_return"
    if leader_return_pct >= threshold_pct:
        return "leader_up"
    if leader_return_pct <= -threshold_pct:
        return "leader_down"
    return "leader_flat"


def pearson(xs, ys):
    if len(xs) < 2 or len(xs) != len(ys):
        return None
    mean_x, mean_y = statistics.mean(xs), statistics.mean(ys)
    numerator = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys))
    denominator_x = math.sqrt(sum((x - mean_x) ** 2 for x in xs))
    denominator_y = math.sqrt(sum((y - mean_y) ** 2 for y in ys))
    return numerator / (denominator_x * denominator_y) if denominator_x and denominator_y else None


def stats(rows):
    returns = [row["follower_forward_return_pct"] for row in rows]
    leader = [row["leader_lookback_return_pct"] for row in rows]
    continuation = [row["follower_forward_return_pct"] * row["leader_lookback_return_pct"]
                    for row in rows if row["leader_lookback_return_pct"] != 0]
    return {
        "observations": len(rows),
        "mean_follower_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_follower_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_follower_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
        "leader_follower_return_correlation": pearson(leader, returns),
        "same_direction_fraction": (sum(value > 0 for value in continuation) / len(continuation)
                                     if continuation else None),
    }


def build_observations(leader, follower, lookback_bars, horizon_bars, threshold_pct):
    timestamps = sorted(set(leader) & set(follower))
    rows = []
    stop = len(timestamps) - horizon_bars
    for index in range(lookback_bars, stop):
        timestamp = timestamps[index]
        leader_return = (leader[timestamp] / leader[timestamps[index - lookback_bars]] - 1.0) * 100.0
        future_timestamp = timestamps[index + horizon_bars]
        follower_return = (follower[future_timestamp] / follower[timestamp] - 1.0) * 100.0
        rows.append({
            "ts_ms": timestamp,
            "future_ts_ms": future_timestamp,
            "leader_lookback_return_pct": leader_return,
            "follower_forward_return_pct": follower_return,
            "state": classify_leader(leader_return, threshold_pct),
        })
    return timestamps, rows


def summarize(rows, min_observations):
    by_state = {state: stats([row for row in rows if row["state"] == state])
                for state in ("leader_up", "leader_down", "leader_flat",
                              "observe_only_missing_leader_return")}
    return {
        "aligned_observations": len(rows),
        "by_state": by_state,
        "verdict": ("cross_asset_lead_lag_response_reported"
                     if len(rows) >= min_observations
                     else "observe_only_insufficient_aligned_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--leader-symbol", default="BTCUSDT")
    parser.add_argument("--follower-symbol", default="ETHUSDT")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=90.0)
    parser.add_argument("--limit", type=int, default=1000)
    parser.add_argument("--lookback-bars", type=int, default=1)
    parser.add_argument("--horizon-bars", type=int, default=1)
    parser.add_argument("--leader-threshold-pct", type=float, default=0.10)
    parser.add_argument("--min-observations", type=int, default=20)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or args.lookback_bars <= 0
            or args.horizon_bars <= 0 or args.leader_threshold_pct < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid days, limit, windows, threshold or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    common = {"exchange": args.exchange, "interval": args.interval,
              "candle_type": "perp", "start_ms": start_ms, "end_ms": end_ms,
              "limit": min(args.limit, 1000)}
    leader_payload = fetch(args.base_url, "/v1/history/candles",
                            {**common, "symbol": args.leader_symbol}, args.timeout)
    follower_payload = fetch(args.base_url, "/v1/history/candles",
                              {**common, "symbol": args.follower_symbol}, args.timeout)
    leader, follower = candle_points(leader_payload), candle_points(follower_payload)
    timestamps, rows = build_observations(
        leader, follower, args.lookback_bars, args.horizon_bars, args.leader_threshold_pct,
    )
    errors = [{"source": source, "error": payload["error"]}
              for source, payload in (("leader", leader_payload), ("follower", follower_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_cross_asset_lead_lag_response_replay",
        "venues": {"exchange": args.exchange, "leader": args.leader_symbol,
                   "follower": args.follower_symbol},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"interval": args.interval, "lookback_bars": args.lookback_bars,
                    "horizon_bars": args.horizon_bars,
                    "leader_threshold_pct": args.leader_threshold_pct,
                    "min_observations": args.min_observations},
        "source_counts": {"leader_bars": len(leader), "follower_bars": len(follower),
                           "exact_intersection_timestamps": len(timestamps)},
        "observations": rows,
        "summary": summarize(rows, args.min_observations),
        "coverage": {"leader": leader_payload.get("coverage_detail"),
                      "follower": follower_payload.get("coverage_detail")},
        "upstream_errors": errors,
        "limitations": [
            "exact timestamp intersection removes unmatched bars but does not remove publication latency",
            "a lead-lag association is not Granger causality, economic significance or a trading edge",
            "close-to-close returns omit fees, funding, slippage, turnover and execution",
            "leader/follower choices and thresholds are caller-supplied research parameters",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
