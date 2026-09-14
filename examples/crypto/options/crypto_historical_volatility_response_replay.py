#!/usr/bin/env python3
"""Replay BTC responses after Bybit provider historical-volatility regimes.

The falsifiable hypothesis is that a high option-market historical-volatility
observation is followed by a different absolute BTC response than ordinary or
low-volatility observations. The provider metric is descriptive context, not
an implied-volatility forecast, option PnL or an execution instruction.
"""

import argparse
import bisect
import json
import statistics
import time

from crypto_funding_spread_response_replay import fetch, price_points


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def volatility_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        volatility = number(row.get("volatility"))
        if isinstance(timestamp, int) and volatility is not None and volatility >= 0:
            points.append((timestamp, volatility * 100.0, row.get("period_days")))
    return sorted(set(points))


def forward_return(points, timestamp, horizon_bars):
    if horizon_bars <= 0 or not points:
        return None
    timestamps = [point[0] for point in points]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if index >= len(points) or future_index >= len(points):
        return None
    baseline = points[index][1]
    future = points[future_index][1]
    if baseline <= 0 or future <= 0:
        return None
    return (future / baseline - 1.0) * 100.0


def classify_state(volatility_pct, low_threshold_pct, high_threshold_pct):
    if volatility_pct is None:
        return "observe_only_missing_volatility"
    if volatility_pct >= high_threshold_pct:
        return "high_provider_historical_volatility"
    if volatility_pct <= low_threshold_pct:
        return "low_provider_historical_volatility"
    return "ordinary_provider_historical_volatility"


def build_observations(volatility, prices, horizon_bars, low_threshold_pct, high_threshold_pct):
    rows = []
    for timestamp, volatility_pct, period_days in volatility:
        future = forward_return(prices, timestamp, horizon_bars)
        rows.append({
            "ts_ms": timestamp,
            "period_days": period_days,
            "provider_historical_volatility_pct": volatility_pct,
            "state": classify_state(volatility_pct, low_threshold_pct, high_threshold_pct),
            "forward_return_pct": future,
            "absolute_forward_return_pct": abs(future) if future is not None else None,
        })
    return rows


def stats(rows):
    signed = [row["forward_return_pct"] for row in rows if row["forward_return_pct"] is not None]
    absolute = [row["absolute_forward_return_pct"] for row in rows
                if row["absolute_forward_return_pct"] is not None]
    return {
        "observations": len(rows),
        "forward_observations": len(signed),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(rows, min_observations, min_edge_bps):
    buckets = {
        state: stats([row for row in rows if row["state"] == state and row["forward_return_pct"] is not None])
        for state in ("high_provider_historical_volatility",
                      "ordinary_provider_historical_volatility",
                      "low_provider_historical_volatility")
    }
    high = buckets["high_provider_historical_volatility"]["mean_absolute_forward_return_pct"]
    ordinary = buckets["ordinary_provider_historical_volatility"]["mean_absolute_forward_return_pct"]
    edge_bps = (high - ordinary) * 100.0 if high is not None and ordinary is not None else None
    return {
        "by_state": buckets,
        "high_minus_ordinary_absolute_response_edge_bps": edge_bps,
        "min_edge_bps": min_edge_bps,
        "verdict": "high_historical_volatility_response_candidate"
        if (buckets["high_provider_historical_volatility"]["observations"] >= min_observations
            and edge_bps is not None and edge_bps >= min_edge_bps)
        else "observe_only",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--base-coin", default="BTC")
    parser.add_argument("--quote-coin", default="USD")
    parser.add_argument("--volatility-exchange", default="bybit")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--price-interval", default="1h")
    parser.add_argument("--period", type=int, default=30)
    parser.add_argument("--days", type=float, default=30.0)
    parser.add_argument("--price-limit", type=int, default=500)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--low-threshold-pct", type=float, default=25.0)
    parser.add_argument("--high-threshold-pct", type=float, default=50.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.period not in (7, 14, 21, 30, 60, 90, 180, 270) or args.days <= 0
            or args.price_limit <= 0 or args.horizon_bars <= 0 or args.low_threshold_pct < 0
            or args.high_threshold_pct <= args.low_threshold_pct or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid period, window, threshold, horizon, observation or timeout argument")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    volatility_payload = fetch(args.base_url, "/v1/history/historical-volatility", {
        "exchange": args.volatility_exchange, "base_coin": args.base_coin,
        "quote_coin": args.quote_coin, "period": args.period,
        "start_ms": start_ms, "end_ms": now_ms,
    }, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.price_exchange, "symbol": args.price_symbol,
        "candle_type": "perp", "interval": args.price_interval,
        "start_ms": start_ms, "end_ms": now_ms, "limit": min(args.price_limit, 1500),
    }, args.timeout)
    volatility = volatility_points(volatility_payload)
    prices = price_points(price_payload)
    rows = build_observations(
        volatility, prices, args.horizon_bars,
        args.low_threshold_pct, args.high_threshold_pct,
    )
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in (("historical_volatility", volatility_payload),
                                    ("price", price_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_historical_volatility_response_replay",
        "base_coin": args.base_coin, "quote_coin": args.quote_coin,
        "volatility_exchange": args.volatility_exchange,
        "price_exchange": args.price_exchange, "price_symbol": args.price_symbol,
        "period_days": args.period,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"low_threshold_pct": args.low_threshold_pct,
                    "high_threshold_pct": args.high_threshold_pct,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"historical_volatility": len(volatility), "price_bars": len(prices)},
        "observations": rows,
        "summary": summarize(rows, args.min_observations, args.min_edge_bps),
        "coverage": {"historical_volatility": volatility_payload.get("coverage_detail"),
                     "price": price_payload.get("coverage_detail")},
        "upstream_errors": errors,
        "limitations": [
            "Bybit historical volatility is a provider option-market metric, not implied volatility or a forecast",
            "price response uses a separate public venue and does not establish causality",
            "forward response excludes option PnL, fees, hedge, slippage and execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
