#!/usr/bin/env python3
"""Replay forward responses after point-in-time crypto drawdowns.

The replay keeps a running high using only candles observed up to each row,
classifies the current close by its drawdown from that high, and measures a
fixed future return, path adverse move, and recovery of the prior high.  It is
a descriptive response study: it does not implement buy-the-dip, DCA,
allocation, or execution.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
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
    return sorted(points.items())


def classify_drawdown(drawdown_pct, mild_drawdown_pct, moderate_drawdown_pct,
                      deep_drawdown_pct):
    if drawdown_pct is None:
        return "observe_only_missing_drawdown"
    if drawdown_pct > -mild_drawdown_pct:
        return "near_ath"
    if drawdown_pct > -moderate_drawdown_pct:
        return "mild_drawdown"
    if drawdown_pct > -deep_drawdown_pct:
        return "moderate_drawdown"
    return "deep_drawdown"


def build_observations(prices, horizon_bars, mild_drawdown_pct,
                       moderate_drawdown_pct, deep_drawdown_pct):
    if horizon_bars <= 0:
        raise ValueError("horizon_bars must be positive")
    if not 0 < mild_drawdown_pct < moderate_drawdown_pct < deep_drawdown_pct:
        raise ValueError("drawdown thresholds must be positive and ordered mild < moderate < deep")
    rows = []
    running_high = None
    for index in range(len(prices) - horizon_bars):
        timestamp, close = prices[index]
        running_high = close if running_high is None else max(running_high, close)
        drawdown_pct = (close / running_high - 1.0) * 100.0
        future_timestamp, future_close = prices[index + horizon_bars]
        path = [prices[offset][1] for offset in range(index + 1, index + horizon_bars + 1)]
        rows.append({
            "ts_ms": timestamp,
            "close": close,
            "running_high": running_high,
            "drawdown_pct": drawdown_pct,
            "state": classify_drawdown(drawdown_pct, mild_drawdown_pct,
                                        moderate_drawdown_pct, deep_drawdown_pct),
            "future_ts_ms": future_timestamp,
            "forward_return_pct": (future_close / close - 1.0) * 100.0,
            "forward_min_path_return_pct": (min(path) / close - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path) / close - 1.0) * 100.0,
            "recovered_prior_high": max(path) >= running_high,
        })
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    adverse = [row["forward_min_path_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "negative_forward_fraction": (sum(value < 0 for value in returns) / len(returns)
                                       if returns else None),
        "mean_forward_min_path_return_pct": statistics.mean(adverse) if adverse else None,
        "recovery_fraction": (sum(row["recovered_prior_high"] for row in rows) / len(rows)
                              if rows else None),
    }


def summarize(observations, min_observations):
    states = ("near_ath", "mild_drawdown", "moderate_drawdown", "deep_drawdown",
              "observe_only_missing_drawdown")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    drawdown_rows = [row for row in observations if row["state"] != "near_ath"
                     and row["state"] != "observe_only_missing_drawdown"]
    return {
        "aligned_forward_windows": len(observations),
        "drawdown_windows": len(drawdown_rows),
        "by_state": by_state,
        "verdict": ("drawdown_recovery_response_reported"
                     if len(drawdown_rows) >= min_observations
                     else "observe_only_insufficient_drawdown_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--days", type=float, default=3650.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--horizon-bars", type=int, default=90)
    parser.add_argument("--mild-drawdown-pct", type=float, default=10.0)
    parser.add_argument("--moderate-drawdown-pct", type=float, default=20.0)
    parser.add_argument("--deep-drawdown-pct", type=float, default=40.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.days <= 0 or not 2 <= args.limit <= 1500 or args.min_observations <= 0:
        parser.error("invalid days, limit or observation count")
    if args.timeout <= 0 or args.horizon_bars <= 0:
        parser.error("invalid timeout or horizon")
    if not 0 < args.mild_drawdown_pct < args.moderate_drawdown_pct < args.deep_drawdown_pct:
        parser.error("drawdown thresholds must be positive and ordered mild < moderate < deep")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    prices = candle_points(payload)
    observations = build_observations(
        prices, args.horizon_bars, args.mild_drawdown_pct,
        args.moderate_drawdown_pct, args.deep_drawdown_pct,
    )
    print(json.dumps({
        "strategy": "crypto_drawdown_recovery_response_replay",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"horizon_bars": args.horizon_bars,
                    "mild_drawdown_pct": args.mild_drawdown_pct,
                    "moderate_drawdown_pct": args.moderate_drawdown_pct,
                    "deep_drawdown_pct": args.deep_drawdown_pct,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(prices),
                           "aligned_forward_windows": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "running high is point-in-time and does not use future candles",
            "drawdown buckets and horizon are caller-supplied descriptive parameters",
            "forward returns omit fees, funding, slippage, liquidity, allocation and execution",
            "recovery of a prior high is a path statistic, not a forecast or buy-the-dip rule",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
