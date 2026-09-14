#!/usr/bin/env python3
"""Replay OHLCV responses after three-candle fair-value-gap proxies.

The proxy requires a non-overlap between candle one and candle three and a
directional middle-candle body.  It then records whether later wicks touch or
fully fill the zone and compares fixed-horizon responses with ordinary bars.
This is an OHLCV event study, not proof of an untraded volume void, intent, or
an executable entry.
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
                and values["high"] >= values["low"] > 0
                and values["high"] >= values["open"]
                and values["high"] >= values["close"]
                and values["low"] <= values["open"]
                and values["low"] <= values["close"]):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def fvg_signal(rows, index, min_gap_bps, min_middle_body_fraction):
    """Return a point-in-time three-candle gap, or None."""
    if index < 2 or min_gap_bps < 0 or not 0 <= min_middle_body_fraction <= 1:
        return None
    first, middle, current = rows[index - 2], rows[index - 1], rows[index]
    middle_range = middle["high"] - middle["low"]
    if middle_range <= 0:
        return None
    body_fraction = abs(middle["close"] - middle["open"]) / middle_range
    if body_fraction < min_middle_body_fraction:
        return None
    bullish_gap = current["low"] - first["high"]
    bearish_gap = first["low"] - current["high"]
    bullish = bullish_gap > 0 and middle["close"] > middle["open"]
    bearish = bearish_gap > 0 and middle["close"] < middle["open"]
    if bullish == bearish:
        return None
    direction = 1 if bullish else -1
    gap = bullish_gap if bullish else bearish_gap
    reference = first["close"]
    gap_bps = gap / reference * 10_000.0 if reference > 0 else 0.0
    if gap_bps < min_gap_bps:
        return None
    zone_low, zone_high = ((first["high"], current["low"])
                           if bullish else (current["high"], first["low"]))
    return {
        "direction": "bullish" if bullish else "bearish",
        "direction_sign": direction,
        "zone_low": zone_low,
        "zone_high": zone_high,
        "gap_bps": gap_bps,
        "middle_body_fraction": body_fraction,
        "first_ts_ms": first["ts_ms"],
        "middle_ts_ms": middle["ts_ms"],
        "formation_ts_ms": current["ts_ms"],
    }


def zone_status(signal, future_rows):
    """Classify future wick interaction with a formed FVG zone."""
    if signal is None:
        return "not_applicable"
    low, high = signal["zone_low"], signal["zone_high"]
    touched = False
    filled = False
    for row in future_rows:
        touched = touched or (row["low"] <= high and row["high"] >= low)
        if signal["direction_sign"] > 0:
            filled = filled or row["low"] <= low
        else:
            filled = filled or row["high"] >= high
    if filled:
        return "wick_filled"
    if touched:
        return "touched"
    return "untouched"


def build_observations(rows, horizon_bars, min_gap_bps, min_middle_body_fraction):
    if horizon_bars <= 0 or min_gap_bps < 0 or not 0 <= min_middle_body_fraction <= 1:
        raise ValueError("invalid horizon, gap or middle-body arguments")
    observations = []
    for index in range(2, len(rows) - horizon_bars):
        signal = fvg_signal(rows, index, min_gap_bps, min_middle_body_fraction)
        future_rows = rows[index + 1:index + horizon_bars + 1]
        future = rows[index + horizon_bars]
        close = rows[index]["close"]
        forward = (future["close"] / close - 1.0) * 100.0
        direction = signal["direction_sign"] if signal else 0
        observations.append({
            "formation_ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "state": signal["direction"] + "_fvg" if signal else "ordinary",
            "signal": signal,
            "zone_status": zone_status(signal, future_rows),
            "formation_close": close,
            "forward_return_pct": forward,
            "direction_aligned_return_bps": direction * forward * 100.0 if direction else None,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(row["low"] for row in future_rows) / close - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(row["high"] for row in future_rows) / close - 1.0) * 100.0,
        })
    return observations


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    aligned = [row["direction_aligned_return_bps"] for row in rows
               if row["direction_aligned_return_bps"] is not None]
    absolute = [row["forward_absolute_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "median_forward_return_pct": statistics.median(signed) if signed else None,
        "mean_direction_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "mean_absolute_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(observations, min_observations):
    states = ("bullish_fvg", "bearish_fvg", "ordinary")
    zone_states = ("untouched", "touched", "wick_filled", "not_applicable")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    by_zone_status = {
        status: bucket_stats([row for row in observations if row["zone_status"] == status])
        for status in zone_states
    }
    event_count = sum(row["state"] != "ordinary" for row in observations)
    ordinary_count = by_state["ordinary"]["observations"]
    sufficient = event_count >= min_observations and ordinary_count >= min_observations
    return {
        "observations": len(observations),
        "fvg_signals": event_count,
        "ordinary_controls": ordinary_count,
        "by_state": by_state,
        "by_zone_status": by_zone_status,
        "verdict": ("fvg_response_reported" if sufficient
                     else "observe_only_insufficient_fvg_or_control"),
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
    parser.add_argument("--min-gap-bps", type=float, default=5.0)
    parser.add_argument("--min-middle-body-fraction", type=float, default=0.50)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.min_gap_bps < 0
            or not 0 <= args.min_middle_body_fraction <= 1 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid gap, body, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.horizon_bars, args.min_gap_bps, args.min_middle_body_fraction,
    )
    print(json.dumps({
        "strategy": "crypto_fair_value_gap_response_replay",
        "hypothesis": "three-candle OHLCV imbalance zones may be revisited and show a different later response from ordinary bars",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"min_gap_bps": args.min_gap_bps,
                    "min_middle_body_fraction": args.min_middle_body_fraction,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations),
                          "fvg_signals": sum(row["state"] != "ordinary" for row in observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "the three-candle wick non-overlap is an OHLCV proxy, not proof of an untraded volume void or institutional intent",
            "gap threshold, middle-body filter and horizon are caller-supplied sensitivity parameters",
            "touch and fill use future candle wicks and do not model path, order priority, fees, funding, slippage or fills",
            "crypto trades continuously, so this is an inter-candle imbalance study rather than a literal session price gap",
            "overlapping event windows are retained and do not establish causality or a trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
