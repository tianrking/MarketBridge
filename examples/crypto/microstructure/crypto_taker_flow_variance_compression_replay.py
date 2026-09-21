#!/usr/bin/env python3
"""Replay compressed taker-flow variance against a later price response.

The source study found pre-cascade compression in taker order-flow variance
across a seven-event BTC perpetual panel. This example is deliberately a
smaller, point-in-time MarketBridge subset: it uses only the bounded historical
taker-volume and candle endpoints, labels a window as compressed relative to a
prior baseline, and reports later absolute returns. It is not a per-event alarm
or a liquidation forecast.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


SOURCE = {
    "title": "Where does the criticality live? Early-warning signals are event-heterogeneous across seven crypto-perpetual liquidation cascades",
    "url": "https://arxiv.org/abs/2607.27070",
    "classification": "directly_verifiable_bounded_subset",
}


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def taker_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        imbalance = number(row.get("imbalance"))
        if isinstance(timestamp, int) and imbalance is not None and -1.0 <= imbalance <= 1.0:
            points.append((timestamp, imbalance))
    return sorted(set(points))


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def rolling_variance(values, window):
    if len(values) < window:
        return None
    return statistics.pvariance(values[-window:])


def compression_rows(points, baseline_bars, window_bars, compression_ratio, prices, horizon_bars):
    rows = []
    for index in range(baseline_bars + window_bars - 1, len(points)):
        current = points[index - window_bars + 1:index + 1]
        baseline = points[index - baseline_bars - window_bars + 1:index - window_bars + 1]
        current_variance = rolling_variance([value for _, value in current], window_bars)
        baseline_variance = rolling_variance([value for _, value in baseline], window_bars)
        if current_variance is None or baseline_variance is None or baseline_variance <= 0:
            continue
        ratio = current_variance / baseline_variance
        timestamp = points[index][0]
        forward = forward_absolute_return(timestamp, prices, horizon_bars)
        rows.append({
            "ts_ms": timestamp,
            "flow_variance": current_variance,
            "baseline_variance": baseline_variance,
            "compression_ratio": ratio,
            "state": "compressed_taker_flow_variance" if ratio <= compression_ratio else "ordinary_taker_flow_variance",
            "forward_absolute_return_pct": forward,
        })
    return rows


def forward_absolute_return(timestamp, points, horizon_bars):
    after = [point for point in points if point[0] >= timestamp]
    if len(after) <= horizon_bars or after[0][1] <= 0:
        return None
    return abs((after[horizon_bars][1] / after[0][1] - 1.0) * 100.0)


def summarize(rows):
    result = {}
    for state in sorted({row["state"] for row in rows}):
        selected = [row for row in rows if row["state"] == state]
        responses = [row["forward_absolute_return_pct"] for row in selected
                     if row["forward_absolute_return_pct"] is not None]
        result[state] = {
            "observations": len(selected),
            "forward_observations": len(responses),
            "mean_forward_absolute_return_pct": statistics.mean(responses) if responses else None,
            "median_forward_absolute_return_pct": statistics.median(responses) if responses else None,
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--period", default="5m")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--baseline-bars", type=int, default=48)
    parser.add_argument("--window-bars", type=int, default=12)
    parser.add_argument("--compression-ratio", type=float, default=0.5)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or args.baseline_bars < args.window_bars
            or args.window_bars < 2 or not 0 < args.compression_ratio < 1
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, compression, horizon, observation or timeout argument")
    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    common = {"symbol": args.symbol, "exchange": args.exchange, "start_ms": start_ms,
              "end_ms": now_ms, "limit": min(args.limit, 500)}
    taker_payload = fetch(args.base_url, "/v1/history/taker-volume",
                          {**common, "period": args.period}, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles",
                          {**common, "candle_type": "perp", "interval": args.period}, args.timeout)
    taker, prices = taker_points(taker_payload), candle_points(price_payload)
    rows = compression_rows(taker, args.baseline_bars, args.window_bars,
                            args.compression_ratio, prices, args.horizon_bars)
    qualifying = [row for row in rows if row["state"] == "compressed_taker_flow_variance"
                  and row["forward_absolute_return_pct"] is not None]
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in (("taker_volume", taker_payload), ("price", price_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_taker_flow_variance_compression_replay",
        "symbol": args.symbol,
        "venue": args.exchange,
        "period": args.period,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"baseline_bars": args.baseline_bars, "window_bars": args.window_bars,
                    "compression_ratio": args.compression_ratio, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"taker_volume": len(taker), "price_bars": len(prices)},
        "observations": rows,
        "summary": summarize(rows),
        "coverage": {"taker_volume": taker_payload.get("coverage_detail"),
                      "price": price_payload.get("coverage_detail")},
        "source": SOURCE,
        "verdict": "compressed-flow response candidate" if len(qualifying) >= args.min_observations else "observe only",
        "upstream_errors": errors,
        "limitations": [
            "This is a single-symbol bounded subset, not the source study's seven-event panel.",
            "Variance compression is a population-level research feature, not a per-event liquidation alarm.",
            "Taker imbalance is an exchange aggregate and not trader identity or intent.",
            "The study's cascade labels, placebo panel, 1-minute prices and 5-minute leverage variables are not reconstructed here.",
            "Forward absolute return is an observation, not a fill, hedge or execution result.",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
