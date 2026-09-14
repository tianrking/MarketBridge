#!/usr/bin/env python3
"""Replay taker buy/sell imbalance plus OI-change states against forward price.

The hypothesis is deliberately split into observable states: aggressive buying
or selling while OI rises (new-position pressure) versus the same flow while OI
falls (possible absorption/position closing). Public aggregate fields do not
identify trader intent, so the result is a descriptive response study only.
"""

import argparse
import json
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
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def taker_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        imbalance = number(row.get("imbalance"))
        if isinstance(timestamp, int) and imbalance is not None and -1.0 <= imbalance <= 1.0:
            points.append((timestamp, imbalance, number(row.get("total_value"))))
    return sorted(set(points))


def oi_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        value = number(row.get("open_interest"))
        if isinstance(timestamp, int) and value is not None and value > 0:
            points.append((timestamp, value))
    return sorted(set(points))


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def oi_change_at(timestamp, points):
    prior = [point for point in points if point[0] <= timestamp]
    if len(prior) < 2 or prior[-2][1] <= 0:
        return None
    return (prior[-1][1] / prior[-2][1] - 1.0) * 100.0


def forward_return(timestamp, points, horizon_bars):
    after = [point for point in points if point[0] >= timestamp]
    if len(after) <= horizon_bars or after[0][1] <= 0:
        return None
    return (after[horizon_bars][1] / after[0][1] - 1.0) * 100.0


def classify_state(imbalance, oi_change_pct, flow_threshold, oi_threshold):
    if imbalance is None or oi_change_pct is None:
        return "observe_only_missing_alignment"
    if imbalance >= flow_threshold and oi_change_pct >= oi_threshold:
        return "buy_pressure_oi_rising"
    if imbalance <= -flow_threshold and oi_change_pct >= oi_threshold:
        return "sell_pressure_oi_rising"
    if imbalance >= flow_threshold and oi_change_pct <= -oi_threshold:
        return "buy_absorption_oi_falling"
    if imbalance <= -flow_threshold and oi_change_pct <= -oi_threshold:
        return "sell_absorption_oi_falling"
    return "ordinary_taker_oi_state"


def summarize(rows):
    states = sorted({row["state"] for row in rows})
    result = {}
    for state in states:
        selected = [row for row in rows if row["state"] == state]
        returns = [row["forward_return_pct"] for row in selected if row["forward_return_pct"] is not None]
        result[state] = {
            "observations": len(selected),
            "forward_observations": len(returns),
            "mean_forward_return_pct": statistics.mean(returns) if returns else None,
            "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns) if returns else None,
            "positive_fraction": sum(value > 0 for value in returns) / len(returns) if returns else None,
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
    parser.add_argument("--flow-threshold", type=float, default=0.20)
    parser.add_argument("--oi-threshold", type=float, default=0.10)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or not 0 <= args.flow_threshold <= 1
            or args.oi_threshold < 0 or args.horizon_bars <= 0 or args.min_observations <= 0
            or args.timeout <= 0):
        parser.error("invalid window, threshold, horizon, observation or timeout argument")
    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    common = {"symbol": args.symbol, "exchange": args.exchange, "start_ms": start_ms,
              "end_ms": now_ms, "limit": min(args.limit, 500)}
    taker_payload = fetch(args.base_url, "/v1/history/taker-volume",
                          {**common, "period": args.period}, args.timeout)
    oi_payload = fetch(args.base_url, "/v1/history/open-interest",
                       {**common, "interval": args.period}, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles",
                          {**common, "candle_type": "perp", "interval": args.period}, args.timeout)
    taker, oi, prices = taker_points(taker_payload), oi_points(oi_payload), candle_points(price_payload)
    rows = []
    for timestamp, imbalance, total_value in taker:
        oi_change = oi_change_at(timestamp, oi)
        rows.append({"ts_ms": timestamp, "taker_imbalance": imbalance, "taker_total_value": total_value,
                     "oi_change_pct": oi_change,
                     "state": classify_state(imbalance, oi_change, args.flow_threshold, args.oi_threshold),
                     "forward_return_pct": forward_return(timestamp, prices, args.horizon_bars)})
    qualifying = [row for row in rows if row["state"] != "observe_only_missing_alignment"
                  and row["forward_return_pct"] is not None]
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in (("taker_volume", taker_payload), ("open_interest", oi_payload), ("price", price_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_taker_oi_response_replay", "symbol": args.symbol,
        "venue": args.exchange, "period": args.period,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"flow_threshold": args.flow_threshold, "oi_threshold": args.oi_threshold,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"taker_volume": len(taker), "open_interest": len(oi), "price_bars": len(prices)},
        "observations": rows, "summary": summarize(rows),
        "coverage": {"taker_volume": taker_payload.get("coverage_detail"),
                      "open_interest": oi_payload.get("coverage_detail"),
                      "price": price_payload.get("coverage_detail")},
        "verdict": "taker-oi response candidate" if len(qualifying) >= args.min_observations else "observe only",
        "upstream_errors": errors,
        "limitations": [
            "taker buy/sell fields are provider aggregates, not trader identity or intent",
            "OI is aggregate positioning and does not identify long/short ownership",
            "forward return is a price observation, not a fill, hedge or execution result",
            "fees, slippage, liquidation, funding cash flow and mark/index divergence are excluded",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
