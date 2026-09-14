#!/usr/bin/env python3
"""Replay BTC response after weekly RSI/SMA crossover states.

This case tests a narrow public-X hypothesis: when weekly RSI crosses below its
own moving average, is the following fixed number of weekly candles different
from ordinary states?  It uses an explicitly documented close-only simple RSI,
keeps the full future path visible, and does not claim a forecast or execute a
trade.
"""

import argparse
import json
import statistics
import time
from datetime import datetime, timezone
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


def utc_date(ts_ms):
    return datetime.fromtimestamp(ts_ms / 1000.0, timezone.utc).date().isoformat()


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            points.append((utc_date(ts_ms), close, ts_ms))
    return sorted({date: (close, ts_ms) for date, close, ts_ms in points}.items())


def simple_rsi(closes, period):
    """Return close-only simple RSI values; unavailable warm-up points are None."""
    if period <= 0:
        raise ValueError("period must be positive")
    values = [None] * len(closes)
    for index in range(period, len(closes)):
        changes = [closes[offset] - closes[offset - 1]
                   for offset in range(index - period + 1, index + 1)]
        gains = [change for change in changes if change > 0]
        losses = [-change for change in changes if change < 0]
        average_gain = sum(gains) / period
        average_loss = sum(losses) / period
        if average_loss == 0:
            values[index] = 100.0 if average_gain > 0 else 50.0
        else:
            values[index] = 100.0 - 100.0 / (1.0 + average_gain / average_loss)
    return values


def classify_cross(previous_rsi, previous_sma, rsi, sma):
    if None in (previous_rsi, previous_sma, rsi, sma):
        return "observe_only_insufficient_rsi_window"
    if previous_rsi >= previous_sma and rsi < sma:
        return "rsi_cross_below_sma"
    if previous_rsi <= previous_sma and rsi > sma:
        return "rsi_cross_above_sma"
    if rsi < sma:
        return "rsi_below_sma"
    if rsi > sma:
        return "rsi_above_sma"
    return "rsi_equal_sma"


def aligned_observations(prices, rsi_period, rsi_sma_period, horizon_weeks):
    if min(rsi_period, rsi_sma_period, horizon_weeks) <= 0:
        raise ValueError("all windows must be positive")
    closes = [row[1][0] for row in prices]
    rsi_values = simple_rsi(closes, rsi_period)
    rows = []
    previous_rsi, previous_sma = None, None
    for index in range(len(prices)):
        if index + horizon_weeks >= len(prices):
            break
        rsi = rsi_values[index]
        if rsi is None or index + 1 < rsi_sma_period:
            previous_rsi, previous_sma = rsi, None
            continue
        recent_rsi = [value for value in rsi_values[index - rsi_sma_period + 1:index + 1]
                      if value is not None]
        if len(recent_rsi) != rsi_sma_period:
            previous_rsi, previous_sma = rsi, None
            continue
        sma = statistics.mean(recent_rsi)
        date, (close, ts_ms) = prices[index]
        future = prices[index + horizon_weeks:index + horizon_weeks + 1][0]
        future_date, (future_close, _) = future
        path_closes = [row[1][0] for row in prices[index + 1:index + horizon_weeks + 1]]
        forward = (future_close / close - 1.0) * 100.0
        rows.append({
            "date": date,
            "ts_ms": ts_ms,
            "close": close,
            "rsi_period": rsi_period,
            "rsi_sma_period": rsi_sma_period,
            "rsi": rsi,
            "rsi_sma": sma,
            "state": classify_cross(previous_rsi, previous_sma, rsi, sma),
            "forward_date": future_date,
            "forward_return_pct": forward,
            "forward_min_path_return_pct": (min(path_closes) / close - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path_closes) / close - 1.0) * 100.0,
        })
        previous_rsi, previous_sma = rsi, sma
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    min_paths = [row["forward_min_path_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_forward_min_path_return_pct": statistics.mean(min_paths) if min_paths else None,
        "worst_forward_min_path_return_pct": min(min_paths) if min_paths else None,
        "negative_forward_fraction": (sum(value < 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize(observations, min_observations):
    states = ("rsi_cross_below_sma", "rsi_cross_above_sma", "rsi_below_sma",
              "rsi_above_sma", "rsi_equal_sma", "observe_only_insufficient_rsi_window")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    crosses = (by_state["rsi_cross_below_sma"]["observations"]
               + by_state["rsi_cross_above_sma"]["observations"])
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "cross_windows": crosses,
        "verdict": ("weekly_rsi_cross_response_reported" if crosses >= min_observations
                     else "observe_only_insufficient_weekly_rsi_crosses"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1w")
    parser.add_argument("--days", type=float, default=3_650.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--rsi-period", type=int, default=14)
    parser.add_argument("--rsi-sma-period", type=int, default=14)
    parser.add_argument("--horizon-weeks", type=int, default=4)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.rsi_period <= 0
            or args.rsi_sma_period <= 0 or args.horizon_weeks <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid days, limit, RSI windows, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    prices = candle_points(payload)
    observations = aligned_observations(prices, args.rsi_period, args.rsi_sma_period,
                                         args.horizon_weeks)
    print(json.dumps({
        "strategy": "crypto_weekly_rsi_cross_response_replay",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"rsi_period": args.rsi_period, "rsi_sma_period": args.rsi_sma_period,
                    "horizon_weeks": args.horizon_weeks, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(prices), "aligned_observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "RSI is a close-only simple-average implementation; provider candles and indicator conventions can differ",
            "a cross is a descriptive event label, not evidence of a 20-30% correction or a forecast",
            "future path statistics are fixed weekly close observations and omit intrabar drawdown, fees, funding and execution",
            "candle retention, missing weeks, multiple-testing and small cross samples remain visible",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
