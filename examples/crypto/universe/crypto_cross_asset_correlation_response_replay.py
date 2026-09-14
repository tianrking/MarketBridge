#!/usr/bin/env python3
"""Replay later relative responses across rolling crypto-correlation regimes.

The replay aligns two candle series on exact timestamps, computes Pearson
correlation on current-and-prior close returns only, and compares the next
fixed-window relative return and absolute relative movement by low, middle, and
high correlation state.  It is a co-movement diagnostic, not a hedge, spread
trade, or causality model.
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


def pearson(xs, ys):
    if len(xs) < 2 or len(xs) != len(ys):
        return None
    mean_x, mean_y = statistics.mean(xs), statistics.mean(ys)
    numerator = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys))
    denominator_x = math.sqrt(sum((x - mean_x) ** 2 for x in xs))
    denominator_y = math.sqrt(sum((y - mean_y) ** 2 for y in ys))
    if not denominator_x or not denominator_y:
        return None
    return numerator / (denominator_x * denominator_y)


def aligned_returns(first, second):
    timestamps = sorted(set(first) & set(second))
    returns = []
    for index in range(1, len(timestamps)):
        previous, current = timestamps[index - 1], timestamps[index]
        if first[previous] <= 0 or second[previous] <= 0:
            continue
        returns.append({
            "ts_ms": current,
            "first_return_pct": (first[current] / first[previous] - 1.0) * 100.0,
            "second_return_pct": (second[current] / second[previous] - 1.0) * 100.0,
        })
    return timestamps, returns


def classify_correlation(correlation, low_threshold, high_threshold):
    if correlation is None:
        return "missing_correlation"
    if correlation <= low_threshold:
        return "low_correlation"
    if correlation >= high_threshold:
        return "high_correlation"
    return "middle_correlation"


def build_observations(first, second, correlation_window, horizon_bars,
                       low_threshold, high_threshold):
    if (correlation_window < 2 or horizon_bars <= 0
            or not -1 <= low_threshold <= 1 or not -1 <= high_threshold <= 1
            or low_threshold >= high_threshold):
        raise ValueError("invalid correlation window, thresholds or horizon")
    timestamps, returns = aligned_returns(first, second)
    if len(timestamps) < 2:
        return timestamps, []
    return_by_ts = {row["ts_ms"]: row for row in returns}
    observations = []
    for return_index in range(correlation_window - 1, len(returns)):
        current_ts = returns[return_index]["ts_ms"]
        try:
            current_index = timestamps.index(current_ts)
        except ValueError:
            continue
        future_index = current_index + horizon_bars
        if future_index >= len(timestamps):
            continue
        window = returns[return_index - correlation_window + 1:return_index + 1]
        correlation = pearson(
            [row["first_return_pct"] for row in window],
            [row["second_return_pct"] for row in window],
        )
        state = classify_correlation(correlation, low_threshold, high_threshold)
        future_ts = timestamps[future_index]
        if current_ts not in first or future_ts not in first or current_ts not in second or future_ts not in second:
            continue
        first_forward = (first[future_ts] / first[current_ts] - 1.0) * 100.0
        second_forward = (second[future_ts] / second[current_ts] - 1.0) * 100.0
        observations.append({
            "ts_ms": current_ts,
            "future_ts_ms": future_ts,
            "correlation": correlation,
            "state": state,
            "first_forward_return_pct": first_forward,
            "second_forward_return_pct": second_forward,
            "relative_forward_return_pct": second_forward - first_forward,
            "absolute_relative_return_pct": abs(second_forward - first_forward),
        })
    return timestamps, observations


def bucket_stats(rows):
    relative = [row["relative_forward_return_pct"] for row in rows]
    absolute = [row["absolute_relative_return_pct"] for row in rows]
    correlations = [row["correlation"] for row in rows if row["correlation"] is not None]
    return {
        "observations": len(rows),
        "mean_correlation": statistics.mean(correlations) if correlations else None,
        "mean_relative_forward_return_pct": statistics.mean(relative) if relative else None,
        "median_relative_forward_return_pct": statistics.median(relative) if relative else None,
        "mean_absolute_relative_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(observations, min_observations, min_abs_relative_edge_bps):
    states = ("low_correlation", "middle_correlation", "high_correlation", "missing_correlation")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    low = by_state["low_correlation"]
    high = by_state["high_correlation"]
    edge_bps = ((low["mean_absolute_relative_return_pct"]
                 - high["mean_absolute_relative_return_pct"]) * 100.0
                if low["mean_absolute_relative_return_pct"] is not None
                and high["mean_absolute_relative_return_pct"] is not None else None)
    sufficient = (low["observations"] >= min_observations
                  and high["observations"] >= min_observations)
    return {
        "aligned_observations": len(observations),
        "low_correlation_observations": low["observations"],
        "high_correlation_observations": high["observations"],
        "absolute_relative_edge_bps_low_minus_high": edge_bps,
        "by_state": by_state,
        "min_observations": min_observations,
        "min_abs_relative_edge_bps": min_abs_relative_edge_bps,
        "verdict": ("cross_asset_correlation_response_reported"
                     if sufficient and edge_bps is not None
                     and edge_bps >= min_abs_relative_edge_bps
                     else "observe_only_insufficient_correlation_or_edge"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--first-symbol", default="BTCUSDT")
    parser.add_argument("--second-symbol", default="ETHUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=365.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--correlation-window", type=int, default=30)
    parser.add_argument("--horizon-bars", type=int, default=6)
    parser.add_argument("--low-correlation", type=float, default=0.30)
    parser.add_argument("--high-correlation", type=float, default=0.70)
    parser.add_argument("--min-observations", type=int, default=20)
    parser.add_argument("--min-absolute-relative-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.correlation_window < 2
            or args.horizon_bars <= 0 or not -1 <= args.low_correlation <= 1
            or not -1 <= args.high_correlation <= 1
            or args.low_correlation >= args.high_correlation or args.min_observations <= 0
            or args.min_absolute_relative_edge_bps < 0 or args.timeout <= 0):
        parser.error("invalid correlation thresholds, windows or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    common = {"exchange": args.exchange, "market": args.market, "candle_type": "perp",
              "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
              "limit": min(args.limit, 1500)}
    first_payload = fetch(args.base_url, "/v1/history/candles",
                          {**common, "symbol": args.first_symbol}, args.timeout)
    second_payload = fetch(args.base_url, "/v1/history/candles",
                           {**common, "symbol": args.second_symbol}, args.timeout)
    first, second = candle_points(first_payload), candle_points(second_payload)
    timestamps, observations = build_observations(
        first, second, args.correlation_window, args.horizon_bars,
        args.low_correlation, args.high_correlation,
    )
    print(json.dumps({
        "strategy": "crypto_cross_asset_correlation_response_replay",
        "hypothesis": "rolling cross-asset correlation regimes may have different later relative-return and dispersion responses",
        "market": {"exchange": args.exchange, "market": args.market,
                   "first_symbol": args.first_symbol, "second_symbol": args.second_symbol,
                   "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"correlation_window": args.correlation_window,
                    "horizon_bars": args.horizon_bars,
                    "low_correlation": args.low_correlation,
                    "high_correlation": args.high_correlation,
                    "min_observations": args.min_observations,
                    "min_absolute_relative_edge_bps": args.min_absolute_relative_edge_bps},
        "source_counts": {"first_bars": len(first), "second_bars": len(second),
                          "exact_intersection_timestamps": len(timestamps),
                          "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations,
                              args.min_absolute_relative_edge_bps),
        "coverage": {"first": first_payload.get("coverage_detail"),
                     "second": second_payload.get("coverage_detail")},
        "upstream_errors": [value for value in (first_payload.get("error"),
                                                  second_payload.get("error")) if value],
        "limitations": [
            "correlation is Pearson correlation of close returns, not a causal or stable dependence model",
            "exact timestamp intersection removes unmatched bars and may reduce coverage",
            "correlation window, regime thresholds and response horizon are caller-supplied sensitivity parameters",
            "relative returns omit hedge ratio, fees, funding, borrow, slippage, turnover and fills",
            "overlapping rolling windows are retained and do not establish a portfolio or trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
