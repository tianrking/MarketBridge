#!/usr/bin/env python3
"""Replay BTC response after a transparent moving-average trend template.

The price-only subset of a public trend-template discussion is tested here:
close above ordered 50/150/200-day averages, a rising 200-day average, and
location near the trailing 52-week high while remaining above the trailing
52-week low.  Fundamental growth and volume criteria are unavailable in this
case and are not silently inferred.
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


def classify_template(close, sma50, sma150, sma200, prior_sma200, trailing_high,
                      trailing_low, near_high_fraction, min_above_low_fraction):
    values = (close, sma50, sma150, sma200, prior_sma200, trailing_high, trailing_low)
    if any(value is None or value <= 0 for value in values):
        return "observe_only_insufficient_trend_window"
    ordered = close > sma50 > sma150 > sma200
    rising = sma200 > prior_sma200
    near_high = close >= trailing_high * near_high_fraction
    above_low = close >= trailing_low * min_above_low_fraction
    if ordered and rising and near_high and above_low:
        return "trend_template_pass"
    if near_high and above_low:
        return "near_high_without_full_template"
    return "ordinary_trend_state"


def aligned_observations(
    prices,
    sma_short_days,
    sma_medium_days,
    sma_long_days,
    slope_days,
    range_days,
    near_high_fraction,
    min_above_low_fraction,
    horizon_days,
):
    windows = (sma_short_days, sma_medium_days, sma_long_days, slope_days, range_days, horizon_days)
    if min(windows) <= 0 or not sma_short_days < sma_medium_days < sma_long_days:
        raise ValueError("windows must be positive and ordered short < medium < long")
    if not 0 < near_high_fraction <= 1 or min_above_low_fraction <= 0:
        raise ValueError("range thresholds are invalid")
    rows = []
    for index in range(sma_long_days - 1, len(prices) - horizon_days):
        date, (close, ts_ms) = prices[index]
        sma = lambda days: statistics.mean(
            prices[offset][1][0] for offset in range(index - days + 1, index + 1)
        )
        sma50, sma150, sma200 = sma(sma_short_days), sma(sma_medium_days), sma(sma_long_days)
        prior_sma200 = statistics.mean(
            prices[offset][1][0]
            for offset in range(index - slope_days - sma_long_days + 1, index - slope_days + 1)
        ) if index >= sma_long_days + slope_days - 1 else None
        trailing = [prices[offset][1][0] for offset in range(index - range_days + 1, index + 1)]
        if len(trailing) != range_days:
            continue
        future_date, (future_close, _) = prices[index + horizon_days]
        path = [prices[offset][1][0] for offset in range(index + 1, index + horizon_days + 1)]
        forward = (future_close / close - 1.0) * 100.0
        rows.append({
            "date": date,
            "ts_ms": ts_ms,
            "close": close,
            "sma_short": sma50,
            "sma_medium": sma150,
            "sma_long": sma200,
            "sma_long_prior": prior_sma200,
            "trailing_high": max(trailing),
            "trailing_low": min(trailing),
            "state": classify_template(close, sma50, sma150, sma200, prior_sma200,
                                        max(trailing), min(trailing), near_high_fraction,
                                        min_above_low_fraction),
            "forward_date": future_date,
            "forward_return_pct": forward,
            "forward_min_path_return_pct": (min(path) / close - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path) / close - 1.0) * 100.0,
        })
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    minima = [row["forward_min_path_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_forward_min_path_return_pct": statistics.mean(minima) if minima else None,
        "negative_forward_fraction": (sum(value < 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize(observations, min_observations):
    states = ("trend_template_pass", "near_high_without_full_template", "ordinary_trend_state",
              "observe_only_insufficient_trend_window")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    passes = by_state["trend_template_pass"]["observations"]
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "template_pass_windows": passes,
        "verdict": ("trend_template_response_reported" if passes >= min_observations
                     else "observe_only_insufficient_trend_template_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--days", type=float, default=1825.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--sma-short-days", type=int, default=50)
    parser.add_argument("--sma-medium-days", type=int, default=150)
    parser.add_argument("--sma-long-days", type=int, default=200)
    parser.add_argument("--slope-days", type=int, default=22)
    parser.add_argument("--range-days", type=int, default=252)
    parser.add_argument("--near-high-fraction", type=float, default=0.75)
    parser.add_argument("--min-above-low-fraction", type=float, default=1.30)
    parser.add_argument("--horizon-days", type=int, default=30)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.days <= 0 or not 2 <= args.limit <= 1500 or args.min_observations <= 0 or args.timeout <= 0:
        parser.error("invalid days, limit, observation count or timeout")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    prices = candle_points(payload)
    observations = aligned_observations(
        prices, args.sma_short_days, args.sma_medium_days, args.sma_long_days,
        args.slope_days, args.range_days, args.near_high_fraction,
        args.min_above_low_fraction, args.horizon_days,
    )
    print(json.dumps({
        "strategy": "crypto_trend_template_response_replay",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"sma_short_days": args.sma_short_days, "sma_medium_days": args.sma_medium_days,
                    "sma_long_days": args.sma_long_days, "slope_days": args.slope_days,
                    "range_days": args.range_days, "near_high_fraction": args.near_high_fraction,
                    "min_above_low_fraction": args.min_above_low_fraction,
                    "horizon_days": args.horizon_days, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(prices), "aligned_observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "only the price/average/range subset is tested; fundamental growth, earnings and sponsorship inputs are unavailable",
            "moving averages and trailing range are descriptive filters, not a universal trend or capacity model",
            "fixed close-to-close responses omit fees, funding, slippage, allocation, causality and execution",
            "candle retention, missing dates, parameter sensitivity and small pass samples remain visible",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
