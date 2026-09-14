#!/usr/bin/env python3
"""Replay forward BTC response by observable price/OI quadrants.

This is a deliberately small decomposition of the public OI/price narrative:
price and aggregate open interest can rise or fall together.  It tests the
four observable quadrants without translating them into long/short ownership,
liquidation certainty, or an execution signal.  OI units remain provider
metadata, so Binance, Bybit, and OKX histories are not silently merged.
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
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def oi_points(payload):
    points = []
    units = set()
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        value = number(row.get("open_interest"))
        if isinstance(timestamp, int) and value is not None and value > 0:
            points.append((timestamp, value))
            if row.get("unit"):
                units.add(str(row["unit"]))
    return sorted(set(points)), sorted(units)


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def latest_index(timestamp, points):
    eligible = [index for index, point in enumerate(points) if point[0] <= timestamp]
    return eligible[-1] if eligible else None


def oi_change_pct(timestamp, points):
    index = latest_index(timestamp, points)
    if index is None or index < 1 or points[index - 1][1] <= 0:
        return None
    return (points[index][1] / points[index - 1][1] - 1.0) * 100.0


def oi_age_ms(timestamp, points):
    index = latest_index(timestamp, points)
    return timestamp - points[index][0] if index is not None else None


def price_return_pct(timestamp, points, lookback_bars):
    index = latest_index(timestamp, points)
    if index is None or index < lookback_bars or points[index - lookback_bars][1] <= 0:
        return None
    return (points[index][1] / points[index - lookback_bars][1] - 1.0) * 100.0


def forward_return_pct(timestamp, points, horizon_bars):
    index = latest_index(timestamp, points)
    if index is None or index + horizon_bars >= len(points) or points[index][1] <= 0:
        return None
    return (points[index + horizon_bars][1] / points[index][1] - 1.0) * 100.0


def classify_quadrant(price_move_pct, oi_delta_pct, min_price_move_pct, min_oi_change_pct):
    if price_move_pct is None or oi_delta_pct is None:
        return "observe_only_missing_price_or_oi"
    price_up = price_move_pct >= min_price_move_pct
    price_down = price_move_pct <= -min_price_move_pct
    oi_up = oi_delta_pct >= min_oi_change_pct
    oi_down = oi_delta_pct <= -min_oi_change_pct
    if price_up and oi_up:
        return "price_up_oi_rising"
    if price_up and oi_down:
        return "price_up_oi_falling"
    if price_down and oi_up:
        return "price_down_oi_rising"
    if price_down and oi_down:
        return "price_down_oi_falling"
    return "flat_or_mixed"


def stats(rows):
    returns = [row["forward_return_pct"] for row in rows if row["forward_return_pct"] is not None]
    return {
        "observations": len(rows),
        "forward_observations": len(returns),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
        "positive_fraction": sum(value > 0 for value in returns) / len(returns)
        if returns else None,
    }


def summarize(rows, min_observations):
    states = ("price_up_oi_rising", "price_up_oi_falling", "price_down_oi_rising",
              "price_down_oi_falling", "flat_or_mixed", "observe_only_missing_price_or_oi")
    by_state = {state: stats([row for row in rows if row["state"] == state])
                for state in states}
    usable = len(rows) - by_state["observe_only_missing_price_or_oi"]["observations"]
    return {
        "by_state": by_state,
        "aligned_observations": len(rows),
        "usable_observations": usable,
        "verdict": ("oi_price_quadrant_response_reported"
                     if usable >= min_observations
                     else "observe_only_insufficient_oi_price_quadrants"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--oi-exchange", default="binance")
    parser.add_argument("--price-interval", default="5m")
    parser.add_argument("--oi-interval", default="5m")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--price-limit", type=int, default=500)
    parser.add_argument("--oi-limit", type=int, default=500)
    parser.add_argument("--lookback-bars", type=int, default=3)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-price-move-pct", type=float, default=0.10)
    parser.add_argument("--min-oi-change-pct", type=float, default=0.10)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.price_limit <= 0 or args.oi_limit <= 0
            or args.lookback_bars <= 0 or args.horizon_bars <= 0
            or args.min_price_move_pct < 0 or args.min_oi_change_pct < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid windows, limits, thresholds or observation count")

    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.price_exchange, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.price_interval,
        "start_ms": start_ms, "end_ms": end_ms, "limit": min(args.price_limit, 1000),
    }, args.timeout)
    oi_payload = fetch(args.base_url, "/v1/history/open-interest", {
        "exchange": args.oi_exchange, "symbol": args.symbol,
        "interval": args.oi_interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.oi_limit, 500),
    }, args.timeout)
    prices = price_points(price_payload)
    oi, oi_units = oi_points(oi_payload)
    rows = []
    for timestamp, _ in prices:
        price_move = price_return_pct(timestamp, prices, args.lookback_bars)
        oi_delta = oi_change_pct(timestamp, oi)
        rows.append({
            "ts_ms": timestamp,
            "lookback_price_return_pct": price_move,
            "oi_change_pct": oi_delta,
            "oi_age_ms": oi_age_ms(timestamp, oi),
            "state": classify_quadrant(price_move, oi_delta, args.min_price_move_pct,
                                         args.min_oi_change_pct),
            "forward_return_pct": forward_return_pct(timestamp, prices, args.horizon_bars),
        })
    errors = [{"source": source, "error": payload["error"]}
              for source, payload in (("price", price_payload), ("open_interest", oi_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_oi_price_divergence_response_replay",
        "symbol": args.symbol,
        "venues": {"price": args.price_exchange, "open_interest": args.oi_exchange},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"price_interval": args.price_interval, "oi_interval": args.oi_interval,
                    "lookback_bars": args.lookback_bars, "horizon_bars": args.horizon_bars,
                    "min_price_move_pct": args.min_price_move_pct,
                    "min_oi_change_pct": args.min_oi_change_pct,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(prices), "open_interest_rows": len(oi)},
        "oi_units": oi_units,
        "observations": rows,
        "summary": summarize(rows, args.min_observations),
        "coverage": {"price": price_payload.get("coverage_detail"),
                      "open_interest": oi_payload.get("coverage_detail")},
        "upstream_errors": errors,
        "limitations": [
            "OI is aggregate provider positioning and does not reveal long/short ownership",
            "quadrant labels are descriptive; they do not prove short covering or liquidation",
            "as-of OI can be stale; inspect oi_age_ms and coverage_detail before interpreting a row",
            "forward return is a price observation, not a fill, hedge or PnL result",
            "fees, funding cash flow, slippage, liquidation and execution are excluded",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
