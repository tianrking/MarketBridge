#!/usr/bin/env python3
"""Replay price/OI/funding positioning regimes from MarketBridge history.

The falsifiable question is descriptive and point-in-time: do combinations of
price trend, open-interest change and funding sign have different forward-return
distributions?  The replay never infers long/short positions from OI alone and
does not convert a regime into an order or hedge.
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


def funding_points(payload):
    schedule = {
        point.get("funding_time_ms"): point.get("interval_ms")
        for point in payload.get("funding_schedule", {}).get("points", [])
        if isinstance(point, dict)
        and isinstance(point.get("funding_time_ms"), int)
        and isinstance(point.get("interval_ms"), int)
        and point["interval_ms"] > 0
    }
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        rate = number(row.get("close"))
        if isinstance(timestamp, int) and rate is not None:
            points.append((timestamp, rate, schedule.get(timestamp)))
    return sorted(set(points))


def oi_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        value = number(row.get("open_interest"))
        if isinstance(timestamp, int) and value is not None and value > 0:
            points.append((timestamp, value))
    return sorted(set(points))


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def _latest_index(timestamp, points):
    eligible = [index for index, point in enumerate(points) if point[0] <= timestamp]
    return eligible[-1] if eligible else None


def oi_change_pct(timestamp, points):
    index = _latest_index(timestamp, points)
    if index is None or index < 1 or points[index - 1][1] <= 0:
        return None
    return (points[index][1] / points[index - 1][1] - 1.0) * 100.0


def lookback_return_pct(timestamp, points, lookback_bars):
    index = _latest_index(timestamp, points)
    if index is None or index < lookback_bars or points[index - lookback_bars][1] <= 0:
        return None
    return (points[index][1] / points[index - lookback_bars][1] - 1.0) * 100.0


def forward_return_pct(timestamp, points, horizon_bars):
    index = _latest_index(timestamp, points)
    if index is None or index + horizon_bars >= len(points) or points[index][1] <= 0:
        return None
    return (points[index + horizon_bars][1] / points[index][1] - 1.0) * 100.0


def _direction(value, threshold, positive, negative):
    if value is None:
        return "missing"
    if value >= threshold:
        return positive
    if value <= -threshold:
        return negative
    return "flat"


def classify_regime(funding_pct, oi_delta_pct, price_return_pct,
                    min_funding_pct, min_oi_change_pct, min_price_move_pct):
    funding = _direction(funding_pct, min_funding_pct, "funding_positive", "funding_negative")
    oi = _direction(oi_delta_pct, min_oi_change_pct, "oi_rising", "oi_falling")
    price = _direction(price_return_pct, min_price_move_pct, "price_up", "price_down")
    if "missing" in (funding, oi, price):
        return "missing_inputs"
    return f"{price}|{oi}|{funding}"


def build_observations(funding, oi, prices, lookback_bars, horizon_bars,
                       min_funding_pct, min_oi_change_pct, min_price_move_pct):
    observations = []
    for timestamp, rate, interval_ms in funding:
        funding_pct = rate * 100.0
        oi_delta = oi_change_pct(timestamp, oi)
        price_return = lookback_return_pct(timestamp, prices, lookback_bars)
        forward = forward_return_pct(timestamp, prices, horizon_bars)
        regime = classify_regime(
            funding_pct, oi_delta, price_return, min_funding_pct,
            min_oi_change_pct, min_price_move_pct,
        )
        direction = 1 if "price_up" in regime else -1 if "price_down" in regime else None
        observations.append({
            "ts_ms": timestamp,
            "funding_rate": rate,
            "funding_pct": funding_pct,
            "funding_interval_ms": interval_ms,
            "oi_change_pct": oi_delta,
            "lookback_price_return_pct": price_return,
            "regime": regime,
            "forward_return_pct": forward,
            "aligned_forward_return_pct": direction * forward if direction is not None and forward is not None else None,
        })
    return observations


def summarize(observations, min_observations):
    by_regime = {}
    for row in observations:
        by_regime.setdefault(row["regime"], []).append(row)
    summary = {}
    for regime, rows in sorted(by_regime.items()):
        returns = [row["forward_return_pct"] for row in rows if row["forward_return_pct"] is not None]
        aligned = [row["aligned_forward_return_pct"] for row in rows
                   if row["aligned_forward_return_pct"] is not None]
        summary[regime] = {
            "observations": len(rows),
            "forward_observations": len(returns),
            "mean_forward_return_pct": statistics.mean(returns) if returns else None,
            "median_forward_return_pct": statistics.median(returns) if returns else None,
            "mean_aligned_forward_return_pct": statistics.mean(aligned) if aligned else None,
            "continuation_hit_rate": sum(value > 0 for value in aligned) / len(aligned) if aligned else None,
            "enough_observations": len(returns) >= min_observations,
        }
    usable = [row for row in observations if row["regime"] != "missing_inputs"]
    return {
        "observations": len(observations),
        "usable_observations": len(usable),
        "missing_input_observations": len(observations) - len(usable),
        "regimes": summary,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--funding-exchange", default="binance")
    parser.add_argument("--oi-exchange", default="binance")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--funding-limit", type=int, default=200)
    parser.add_argument("--oi-limit", type=int, default=500)
    parser.add_argument("--price-limit", type=int, default=500)
    parser.add_argument("--price-interval", default="5m")
    parser.add_argument("--lookback-bars", type=int, default=3)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-funding-pct", type=float, default=0.01)
    parser.add_argument("--min-oi-change-pct", type=float, default=0.10)
    parser.add_argument("--min-price-move-pct", type=float, default=0.10)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.days <= 0 or min(options.funding_limit, options.oi_limit, options.price_limit) <= 0
            or options.lookback_bars <= 0 or options.horizon_bars <= 0
            or min(options_f for options_f in (options.min_funding_pct, options.min_oi_change_pct,
                                               options.min_price_move_pct)) < 0
            or options.min_observations <= 0):
        parser.error("invalid windows, limits, thresholds or observation count")

    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(options.days * 86_400_000)
    common = {"symbol": options.symbol, "start_ms": start_ms, "end_ms": end_ms}
    funding_payload = fetch(options.base_url, "/v1/history/candles", {
        **common, "exchange": options.funding_exchange, "candle_type": "funding_rate",
        "limit": min(options.funding_limit, 500),
    }, options.timeout)
    oi_payload = fetch(options.base_url, "/v1/history/open-interest", {
        **common, "exchange": options.oi_exchange, "interval": options.price_interval,
        "limit": min(options.oi_limit, 500),
    }, options.timeout)
    price_payload = fetch(options.base_url, "/v1/history/candles", {
        **common, "exchange": options.price_exchange, "candle_type": "perp",
        "interval": options.price_interval, "limit": min(options.price_limit, 1000),
    }, options.timeout)
    funding = funding_points(funding_payload)
    oi = oi_points(oi_payload)
    prices = price_points(price_payload)
    observations = build_observations(
        funding, oi, prices, options.lookback_bars, options.horizon_bars,
        options.min_funding_pct, options.min_oi_change_pct, options.min_price_move_pct,
    )
    errors = []
    for source, payload in (("funding", funding_payload), ("open_interest", oi_payload), ("price", price_payload)):
        if payload.get("error"):
            errors.append({"source": source, "error": payload["error"]})
    evidence = [
        "funding_history_available" if funding else "missing_funding_history",
        "open_interest_history_available" if oi else "missing_open_interest_history",
        "price_history_available" if prices else "missing_price_history",
    ]
    if any(row["regime"] == "missing_inputs" for row in observations):
        evidence.append("some_regime_inputs_missing")
    print(json.dumps({
        "strategy": "crypto_positioning_regime_replay",
        "hypothesis": "price/OI quadrant and funding sign may separate forward-return distributions",
        "symbol": options.symbol,
        "venues": {"funding": options.funding_exchange, "open_interest": options.oi_exchange,
                   "price": options.price_exchange},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": options.days},
        "filters": {"lookback_bars": options.lookback_bars, "horizon_bars": options.horizon_bars,
                    "min_funding_pct": options.min_funding_pct, "min_oi_change_pct": options.min_oi_change_pct,
                    "min_price_move_pct": options.min_price_move_pct,
                    "min_observations": options.min_observations},
        "source_counts": {"funding": len(funding), "open_interest": len(oi), "price_bars": len(prices)},
        "observations": observations,
        "summary": summarize(observations, options.min_observations),
        "evidence": evidence,
        "upstream_errors": errors,
        "limitations": [
            "OI is aggregate positioning and does not identify long/short ownership",
            "funding sign is a crowding proxy, not a causal direction label",
            "point-in-time joins keep missing timestamps visible and do not fill with zero",
            "forward return is a price observation, not a hedge, fill or PnL result",
            "fees, borrow, margin, slippage, liquidation and execution are excluded",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
