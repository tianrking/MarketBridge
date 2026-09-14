#!/usr/bin/env python3
"""Replay OHLCV responses after a prior-range sweep and reclaim.

This is the observable subset of a liquidity-sweep/reclaim discussion: the
current candle trades beyond a prior lookback high/low and closes back through
that level with a directional body.  It does not claim to observe resting stop
orders, a true liquidity pool, CISD, displacement intent, or execution.
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
                and values["low"] > 0 and values["high"] >= values["low"]
                and values["open"] > 0 and values["close"] > 0):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def prior_range(rows, index, lookback_bars):
    if lookback_bars <= 0 or index < lookback_bars:
        return None
    prior = rows[index - lookback_bars:index]
    return {
        "high": max(row["high"] for row in prior),
        "low": min(row["low"] for row in prior),
        "bars": lookback_bars,
    }


def sweep_signal(row, range_levels, sweep_buffer, min_body_fraction, min_range_bps):
    if range_levels is None:
        return None
    candle_range = row["high"] - row["low"]
    if candle_range <= 0 or row["open"] <= 0:
        return None
    body_fraction = abs(row["close"] - row["open"]) / candle_range
    range_bps = candle_range / row["open"] * 10_000.0
    if body_fraction < min_body_fraction or range_bps < min_range_bps:
        return None
    bullish = (row["low"] < range_levels["low"] * (1.0 - sweep_buffer)
               and row["close"] > range_levels["low"]
               and row["close"] > row["open"])
    bearish = (row["high"] > range_levels["high"] * (1.0 + sweep_buffer)
               and row["close"] < range_levels["high"]
               and row["close"] < row["open"])
    if bullish == bearish:
        return None
    direction = 1 if bullish else -1
    level = range_levels["low"] if bullish else range_levels["high"]
    extreme = row["low"] if bullish else row["high"]
    return {
        "direction": "bullish" if bullish else "bearish",
        "direction_sign": direction,
        "sweep_level": level,
        "sweep_extreme": extreme,
        "body_fraction": body_fraction,
        "range_bps": range_bps,
        "reclaim_distance_bps": direction * (row["close"] / level - 1.0) * 10_000.0,
        "prior_range_high": range_levels["high"],
        "prior_range_low": range_levels["low"],
    }


def build_observations(rows, lookback_bars, horizon_bars, sweep_buffer_bps,
                       min_body_fraction, min_range_bps):
    if lookback_bars <= 0 or horizon_bars <= 0:
        raise ValueError("lookback_bars and horizon_bars must be positive")
    observations = []
    for index in range(lookback_bars, len(rows) - horizon_bars):
        levels = prior_range(rows, index, lookback_bars)
        signal = sweep_signal(rows[index], levels, sweep_buffer_bps / 10_000.0,
                              min_body_fraction, min_range_bps)
        if signal is None:
            continue
        future = rows[index + horizon_bars]
        path = [rows[offset]["close"] for offset in range(index + 1, index + horizon_bars + 1)]
        forward_return_pct = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = signal["direction_sign"]
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "trigger_close": rows[index]["close"],
            "signal": signal,
            "forward_return_pct": forward_return_pct,
            "aligned_return_bps": direction * forward_return_pct * 100.0,
            "forward_min_path_return_pct": (min(path) / rows[index]["close"] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path) / rows[index]["close"] - 1.0) * 100.0,
            "aligned": direction * forward_return_pct > 0,
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
        "bullish_signals": sum(row["signal"]["direction_sign"] > 0 for row in observations),
        "bearish_signals": sum(row["signal"]["direction_sign"] < 0 for row in observations),
        "aligned_hit_rate": (sum(row["aligned"] for row in observations) / len(observations)
                              if observations else None),
        "mean_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "median_aligned_return_bps": statistics.median(aligned) if aligned else None,
        "mean_cost_adjusted_return_bps": mean_adjusted,
        "paper_cost_bps": paper_cost_bps,
        "min_edge_bps": min_edge_bps,
        "verdict": "liquidity_sweep_response_reported" if candidate else "observe_only",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="15m")
    parser.add_argument("--days", type=float, default=15.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--lookback-bars", type=int, default=20)
    parser.add_argument("--sweep-buffer-bps", type=float, default=0.0)
    parser.add_argument("--min-body-fraction", type=float, default=0.50)
    parser.add_argument("--min-range-bps", type=float, default=5.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.lookback_bars <= 0
            or args.horizon_bars <= 0 or args.sweep_buffer_bps < 0
            or not 0 <= args.min_body_fraction <= 1 or args.min_range_bps < 0
            or args.paper_cost_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid sweep, candle, cost or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.lookback_bars, args.horizon_bars, args.sweep_buffer_bps,
        args.min_body_fraction, args.min_range_bps,
    )
    print(json.dumps({
        "strategy": "crypto_liquidity_sweep_response_replay",
        "hypothesis": "a prior-range sweep followed by a close reclaim and directional body may separate later aligned returns",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"lookback_bars": args.lookback_bars,
                    "sweep_buffer_bps": args.sweep_buffer_bps,
                    "min_body_fraction": args.min_body_fraction,
                    "min_range_bps": args.min_range_bps,
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
            "prior range is an OHLCV proxy and does not observe resting stops or a latent liquidity pool",
            "reclaim, body and range thresholds are caller-supplied sensitivity parameters",
            "candle-row horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "the replay does not implement CISD, displacement intent, order routing or position management",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
