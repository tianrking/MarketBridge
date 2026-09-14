#!/usr/bin/env python3
"""Replay OHLCV responses after a range breakout and level retest.

The replay identifies a close outside a prior lookback range, then requires a
later candle inside a bounded window to touch the broken level and close back
on the breakout side.  The response starts at that retest close.  This is an
OHLCV event study, not proof of support/resistance, a successful trade, or an
execution/fill model.
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


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        values = {key: number(row.get(key)) for key in ("open", "high", "low", "close")}
        timestamp = row.get("open_time_ms")
        if (isinstance(timestamp, int) and all(value is not None for value in values.values())
                and values["open"] > 0 and values["close"] > 0
                and values["high"] >= values["low"] > 0):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def prior_levels(rows, index, lookback_bars):
    if lookback_bars <= 0 or index < lookback_bars:
        return None
    window = rows[index - lookback_bars:index]
    return {"resistance": max(row["high"] for row in window),
            "support": min(row["low"] for row in window)}


def breakout_direction(row, previous_close, levels, breakout_buffer):
    if levels is None:
        return 0, None
    bullish = (previous_close <= levels["resistance"]
               and row["close"] > levels["resistance"] * (1.0 + breakout_buffer))
    bearish = (previous_close >= levels["support"]
               and row["close"] < levels["support"] * (1.0 - breakout_buffer))
    if bullish == bearish:
        return 0, None
    return (1, levels["resistance"]) if bullish else (-1, levels["support"])


def find_retest(rows, breakout_index, direction, level, retest_window, tolerance):
    stop = min(len(rows), breakout_index + retest_window + 1)
    for index in range(breakout_index + 1, stop):
        row = rows[index]
        if direction > 0:
            touched = row["low"] <= level * (1.0 + tolerance)
            reclaimed = row["close"] > level
        else:
            touched = row["high"] >= level * (1.0 - tolerance)
            reclaimed = row["close"] < level
        if touched and reclaimed:
            return index
    return None


def build_observations(rows, lookback_bars, retest_window, retest_tolerance_bps,
                       breakout_buffer_bps, horizon_bars):
    if lookback_bars <= 0 or retest_window <= 0 or horizon_bars <= 0:
        raise ValueError("lookback, retest window and horizon must be positive")
    observations = []
    for index in range(lookback_bars, len(rows) - 1):
        levels = prior_levels(rows, index, lookback_bars)
        direction, level = breakout_direction(
            rows[index], rows[index - 1]["close"], levels, breakout_buffer_bps / 10_000.0,
        )
        if direction == 0:
            continue
        retest_index = find_retest(
            rows, index, direction, level, retest_window, retest_tolerance_bps / 10_000.0,
        )
        if retest_index is None or retest_index + horizon_bars >= len(rows):
            continue
        trigger, retest, future = rows[index], rows[retest_index], rows[retest_index + horizon_bars]
        path = [rows[offset]["close"]
                for offset in range(retest_index + 1, retest_index + horizon_bars + 1)]
        forward = (future["close"] / retest["close"] - 1.0) * 100.0
        observations.append({
            "breakout_ts_ms": trigger["ts_ms"],
            "retest_ts_ms": retest["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "direction": "bullish" if direction > 0 else "bearish",
            "direction_sign": direction,
            "breakout_level": level,
            "breakout_close": trigger["close"],
            "retest_low": retest["low"],
            "retest_high": retest["high"],
            "retest_close": retest["close"],
            "retest_distance_bps": direction * (retest["close"] / level - 1.0) * 10_000.0,
            "forward_return_pct": forward,
            "aligned_return_bps": direction * forward * 100.0,
            "forward_min_path_return_pct": (min(path) / retest["close"] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path) / retest["close"] - 1.0) * 100.0,
            "aligned": direction * forward > 0,
        })
    return observations


def summarize(observations, paper_cost_bps, min_observations, min_edge_bps):
    aligned = [row["aligned_return_bps"] for row in observations]
    adjusted = [value - paper_cost_bps for value in aligned]
    mean_adjusted = statistics.mean(adjusted) if adjusted else None
    candidate = (len(adjusted) >= min_observations and mean_adjusted is not None
                 and mean_adjusted >= min_edge_bps)
    return {
        "signals": len(observations),
        "bullish_retests": sum(row["direction_sign"] > 0 for row in observations),
        "bearish_retests": sum(row["direction_sign"] < 0 for row in observations),
        "aligned_hit_rate": (sum(row["aligned"] for row in observations) / len(observations)
                              if observations else None),
        "mean_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "median_aligned_return_bps": statistics.median(aligned) if aligned else None,
        "mean_cost_adjusted_return_bps": mean_adjusted,
        "paper_cost_bps": paper_cost_bps,
        "min_edge_bps": min_edge_bps,
        "verdict": "breakout_retest_response_reported" if candidate else "observe_only",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=90.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--lookback-bars", type=int, default=24)
    parser.add_argument("--breakout-buffer-bps", type=float, default=2.0)
    parser.add_argument("--retest-window", type=int, default=8)
    parser.add_argument("--retest-tolerance-bps", type=float, default=15.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.lookback_bars <= 0
            or args.breakout_buffer_bps < 0 or args.retest_window <= 0
            or args.retest_tolerance_bps < 0 or args.horizon_bars <= 0
            or args.paper_cost_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid breakout, retest, horizon, cost or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.lookback_bars, args.retest_window, args.retest_tolerance_bps,
        args.breakout_buffer_bps, args.horizon_bars,
    )
    print(json.dumps({
        "strategy": "crypto_breakout_retest_response_replay",
        "hypothesis": "a range breakout followed by a bounded touch-and-reclaim retest may separate later aligned returns",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"lookback_bars": args.lookback_bars,
                    "breakout_buffer_bps": args.breakout_buffer_bps,
                    "retest_window": args.retest_window,
                    "retest_tolerance_bps": args.retest_tolerance_bps,
                    "horizon_bars": args.horizon_bars,
                    "paper_cost_bps": args.paper_cost_bps,
                    "min_edge_bps": args.min_edge_bps,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "signals": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.paper_cost_bps,
                              args.min_observations, args.min_edge_bps),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "range levels and retest are OHLCV proxies, not proof of support, resistance or resting orders",
            "breakout, retest tolerance, windows and cost hurdle are caller-supplied parameters",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "overlapping breakouts are retained as observations and no order, wallet or position path exists",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
