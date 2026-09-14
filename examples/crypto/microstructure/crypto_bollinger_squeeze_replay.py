#!/usr/bin/env python3
"""Replay Bollinger BandWidth squeeze-breakout responses from candle history."""

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
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close, high, low = (number(row.get(key)) for key in ("close", "high", "low"))
        if (isinstance(timestamp, int) and close is not None and close > 0
                and high is not None and low is not None and high >= low > 0):
            rows.append({"ts_ms": timestamp, "close": close, "high": high, "low": low})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def band_stats(closes, end_index, period, deviations):
    start = end_index - period + 1
    if start < 0 or end_index >= len(closes):
        return None
    window = closes[start:end_index + 1]
    if len(window) != period or any(value <= 0 for value in window):
        return None
    middle = statistics.mean(window)
    spread = statistics.pstdev(window) * deviations
    if middle <= 0:
        return None
    upper, lower = middle + spread, middle - spread
    return {
        "middle": middle, "upper": upper, "lower": lower,
        "bandwidth_pct": (upper - lower) / middle * 100.0,
    }


def percentile(values, quantile):
    if not values:
        return None
    ordered = sorted(values)
    position = (len(ordered) - 1) * quantile
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def squeeze_breakout(rows, index, period, deviations, bandwidth_lookback,
                     max_bandwidth_quantile, breakout_buffer):
    """Use only widths before the trigger and the prior candle's bands."""
    if index < period + bandwidth_lookback:
        return None
    closes = [row["close"] for row in rows]
    prior_index = index - 1
    prior = band_stats(closes, prior_index, period, deviations)
    if prior is None:
        return None
    historical = []
    for offset in range(prior_index - bandwidth_lookback, prior_index):
        point = band_stats(closes, offset, period, deviations)
        if point is not None:
            historical.append(point["bandwidth_pct"])
    threshold = percentile(historical, max_bandwidth_quantile)
    if threshold is None or prior["bandwidth_pct"] > threshold:
        return None
    close = rows[index]["close"]
    direction = (1 if close > prior["upper"] * (1.0 + breakout_buffer)
                 else -1 if close < prior["lower"] * (1.0 - breakout_buffer) else 0)
    return {
        "direction": direction,
        "prior_bandwidth_pct": prior["bandwidth_pct"],
        "historical_bandwidth_quantile_pct": threshold,
        "prior_middle": prior["middle"], "prior_upper": prior["upper"],
        "prior_lower": prior["lower"], "trigger_close": close,
        "squeeze": True,
    }


def forward_return(rows, index, horizon_bars):
    target = index + horizon_bars
    if target >= len(rows) or rows[index]["close"] <= 0:
        return None
    return (rows[target]["close"] / rows[index]["close"] - 1.0) * 100.0


def summarize(events, paper_cost_bps, min_edge_bps, min_observations):
    valid = [event for event in events if event.get("forward_return_pct") is not None]
    aligned = [event["direction_sign"] * event["forward_return_pct"] * 100.0 for event in valid]
    adjusted = [value - paper_cost_bps for value in aligned]
    mean_adjusted = statistics.mean(adjusted) if adjusted else None
    candidate = (len(adjusted) >= min_observations and mean_adjusted is not None
                 and mean_adjusted >= min_edge_bps)
    return {
        "events": len(events), "forward_observations": len(valid),
        "up_breakouts": sum(event["direction_sign"] > 0 for event in events),
        "down_breakouts": sum(event["direction_sign"] < 0 for event in events),
        "directional_hit_rate": (sum(value > 0 for value in aligned) / len(aligned)
                                  if aligned else None),
        "mean_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "median_aligned_return_bps": statistics.median(aligned) if aligned else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_return_bps": mean_adjusted,
        "min_edge_bps": min_edge_bps,
        "verdict": "bollinger_squeeze_response_reported" if candidate else "observe_only",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--period", type=int, default=20)
    parser.add_argument("--deviations", type=float, default=2.0)
    parser.add_argument("--bandwidth-lookback", type=int, default=96)
    parser.add_argument("--max-bandwidth-quantile", type=float, default=0.20)
    parser.add_argument("--breakout-buffer-bps", type=float, default=0.0)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 1 <= args.limit <= 1500 or args.period < 2
            or args.deviations <= 0 or args.bandwidth_lookback < 2
            or not 0 < args.max_bandwidth_quantile <= 1 or args.breakout_buffer_bps < 0
            or args.horizon_bars <= 0 or args.paper_cost_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid Bollinger, horizon, cost or history arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "symbol": args.symbol, "market": args.market,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    events = []
    for index in range(max(args.period + args.bandwidth_lookback, 1),
                       max(args.period + args.bandwidth_lookback, len(rows) - args.horizon_bars)):
        features = squeeze_breakout(
            rows, index, args.period, args.deviations, args.bandwidth_lookback,
            args.max_bandwidth_quantile, args.breakout_buffer_bps / 10_000.0,
        )
        if features is None or features["direction"] == 0:
            continue
        forward = forward_return(rows, index, args.horizon_bars)
        events.append({
            "ts_ms": rows[index]["ts_ms"],
            "direction": "up" if features["direction"] > 0 else "down",
            "direction_sign": features["direction"],
            "features": features,
            "forward_return_pct": forward,
        })
    coverage = payload.get("coverage_detail")
    evidence = ["historical_candles_available" if rows else "missing_historical_candles"]
    if isinstance(coverage, dict) and coverage.get("status"):
        evidence.append(f"candle_coverage_{coverage['status']}")
    print(json.dumps({
        "strategy": "crypto_bollinger_squeeze_replay",
        "hypothesis": "a prior BandWidth squeeze followed by a band break may precede directional continuation",
        "market": {"exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                   "interval": args.interval},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"period": args.period, "deviations": args.deviations,
                    "bandwidth_lookback": args.bandwidth_lookback,
                    "max_bandwidth_quantile": args.max_bandwidth_quantile,
                    "breakout_buffer_bps": args.breakout_buffer_bps,
                    "horizon_bars": args.horizon_bars, "paper_cost_bps": args.paper_cost_bps,
                    "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "squeeze_breakouts": len(events)},
        "coverage": coverage, "observations": events,
        "summary": summarize(events, args.paper_cost_bps, args.min_edge_bps, args.min_observations),
        "evidence": evidence,
        "upstream_errors": [payload.get("error")] if payload.get("error") else [],
        "limitations": [
            "BandWidth uses close-only population standard deviation and caller-selected parameters",
            "the squeeze threshold is a trailing empirical quantile, not a universal level",
            "candle history is bounded and forward close-to-close returns are not fills",
            "paper cost is a sensitivity hurdle, not fees, slippage, leverage, ATR stops or execution",
            "overlapping events and venue-specific candle coverage can affect independence",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
