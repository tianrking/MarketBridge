#!/usr/bin/env python3
"""Replay a volume-profile, VWAP and open-interest confluence hypothesis.

The public research lead is intentionally reduced to observable data: a close
outside a prior OHLCV value area, on the same side of a trailing VWAP, with a
fresh OI change in the matching direction.  The case compares those states
with explicit non-confluence controls over a fixed forward candle horizon.  It
is a response study, not an order, fill or position-management model.
"""

import argparse
import bisect
import json
import re
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


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        values = {key: number(row.get(key)) for key in ("close", "high", "low", "volume")}
        if (isinstance(timestamp, int) and values["close"] is not None
                and values["high"] is not None and values["low"] is not None
                and values["volume"] is not None and values["close"] > 0
                and values["high"] >= values["low"] > 0
                and values["high"] >= values["close"] >= values["low"]
                and values["volume"] >= 0):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def oi_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        value = number(row.get("open_interest"))
        if isinstance(timestamp, int) and value is not None and value > 0:
            points.append((timestamp, value))
    return sorted(set(points))


def interval_millis(value):
    match = re.fullmatch(r"(\d+)([smhdw])", value.strip())
    if match is None:
        return None
    multipliers = {"s": 1_000, "m": 60_000, "h": 3_600_000,
                   "d": 86_400_000, "w": 604_800_000}
    return int(match.group(1)) * multipliers[match.group(2)]


def oi_change_at(points, timestamp, lookback_ms, max_age_ms):
    if not points or lookback_ms <= 0 or max_age_ms < 0:
        return None
    timestamps = [point[0] for point in points]
    current_index = bisect.bisect_right(timestamps, timestamp) - 1
    if current_index < 0:
        return None
    current_ts, current_value = points[current_index]
    age_ms = timestamp - current_ts
    if age_ms > max_age_ms or current_value <= 0:
        return None
    prior_index = bisect.bisect_right(timestamps, current_ts - lookback_ms) - 1
    if prior_index < 0 or prior_index >= current_index:
        return None
    prior_ts, prior_value = points[prior_index]
    if prior_value <= 0:
        return None
    return {
        "current_ts_ms": current_ts,
        "previous_ts_ms": prior_ts,
        "age_ms": age_ms,
        "change_pct": (current_value / prior_value - 1.0) * 100.0,
    }


def percentile(values, fraction):
    ordered = sorted(values)
    if not ordered:
        return None
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def profile_vwap_features(rows, index, lookback_bars, bins, value_area_fraction,
                          low_volume_quantile):
    if index < lookback_bars or bins < 2:
        return None
    prior = rows[index - lookback_bars:index]
    profile_low = min(row["low"] for row in prior)
    profile_high = max(row["high"] for row in prior)
    if not profile_high > profile_low:
        return None
    width = (profile_high - profile_low) / bins
    volumes = [0.0] * bins
    cumulative_value = 0.0
    cumulative_volume = 0.0
    for row in prior:
        typical = (row["high"] + row["low"] + row["close"]) / 3.0
        volume = max(0.0, row["volume"])
        bucket = min(bins - 1, max(0, int((typical - profile_low) / width)))
        volumes[bucket] += volume
        cumulative_value += typical * volume
        cumulative_volume += volume
    if cumulative_volume <= 0:
        return None
    poc_bucket = max(range(bins), key=volumes.__getitem__)
    selected = {poc_bucket}
    covered = volumes[poc_bucket]
    target = cumulative_volume * value_area_fraction
    while covered < target and len(selected) < bins:
        candidates = [candidate for candidate in (min(selected) - 1, max(selected) + 1)
                      if 0 <= candidate < bins and candidate not in selected]
        if not candidates:
            break
        candidate = max(candidates, key=volumes.__getitem__)
        selected.add(candidate)
        covered += volumes[candidate]
    value_low = profile_low + min(selected) * width
    value_high = profile_low + (max(selected) + 1) * width
    threshold = percentile(volumes, low_volume_quantile)
    current_close = rows[index]["close"]
    return {
        "profile_low": profile_low,
        "profile_high": profile_high,
        "poc": profile_low + (poc_bucket + 0.5) * width,
        "value_low": value_low,
        "value_high": value_high,
        "vwap": cumulative_value / cumulative_volume,
        "current_close": current_close,
        "low_volume_threshold": threshold,
        "volume_total": cumulative_volume,
        "bins": bins,
        "lookback_bars": lookback_bars,
    }


def classify_state(close, features, oi_change, min_oi_change_pct):
    if features is None:
        return "missing_profile"
    bullish_alignment = close > features["value_high"] and close > features["vwap"]
    bearish_alignment = close < features["value_low"] and close < features["vwap"]
    if oi_change is None:
        return "missing_oi"
    if bullish_alignment and oi_change >= min_oi_change_pct:
        return "bullish_profile_vwap_oi"
    if bearish_alignment and oi_change <= -min_oi_change_pct:
        return "bearish_profile_vwap_oi"
    if bullish_alignment or bearish_alignment:
        return "profile_vwap_aligned_without_oi"
    if features["value_low"] <= close <= features["value_high"]:
        return "inside_value_area"
    return "profile_vwap_disagreement"


def build_observations(rows, oi, lookback_bars, bins, value_area_fraction,
                       low_volume_quantile, min_oi_change_pct, oi_lookback_ms,
                       max_oi_age_ms, horizon_bars):
    if (lookback_bars < 2 or bins < 2 or not 0 < value_area_fraction <= 1
            or not 0 <= low_volume_quantile <= 1 or min_oi_change_pct < 0
            or oi_lookback_ms <= 0 or max_oi_age_ms < 0 or horizon_bars <= 0):
        raise ValueError("invalid profile, OI, horizon or sensitivity arguments")
    observations = []
    for index in range(len(rows) - horizon_bars):
        features = profile_vwap_features(rows, index, lookback_bars, bins,
                                         value_area_fraction, low_volume_quantile)
        oi_context = oi_change_at(oi, rows[index]["ts_ms"], oi_lookback_ms, max_oi_age_ms)
        oi_change = oi_context["change_pct"] if oi_context is not None else None
        state = classify_state(rows[index]["close"], features, oi_change,
                               min_oi_change_pct)
        future = rows[index + horizon_bars]
        path = rows[index + 1:index + horizon_bars + 1]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = (1 if state == "bullish_profile_vwap_oi" else
                     -1 if state == "bearish_profile_vwap_oi" else 0)
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "state": state,
            "direction_sign": direction,
            "oi": oi_context,
            "features": features,
            "forward_return_pct": forward,
            "direction_aligned_return_bps": direction * forward * 100.0 if direction else None,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(row["low"] for row in path)
                                             / rows[index]["close"] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(row["high"] for row in path)
                                             / rows[index]["close"] - 1.0) * 100.0,
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
    states = ("bullish_profile_vwap_oi", "bearish_profile_vwap_oi",
              "profile_vwap_aligned_without_oi", "inside_value_area",
              "profile_vwap_disagreement", "missing_oi", "missing_profile")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    confluence = [row for row in observations
                  if row["state"] in {"bullish_profile_vwap_oi", "bearish_profile_vwap_oi"}]
    control_states = {"profile_vwap_aligned_without_oi", "inside_value_area",
                      "profile_vwap_disagreement"}
    controls = [row for row in observations if row["state"] in control_states]
    sufficient = len(confluence) >= min_observations and len(controls) >= min_observations
    return {
        "observations": len(observations),
        "confluence_observations": len(confluence),
        "control_observations": len(controls),
        "by_state": by_state,
        "confluence": bucket_stats(confluence),
        "controls": bucket_stats(controls),
        "verdict": ("profile_vwap_oi_confluence_reported" if sufficient
                     else "observe_only_insufficient_confluence_or_control"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--oi-exchange", default="binance")
    parser.add_argument("--oi-interval", default="5m")
    parser.add_argument("--days", type=float, default=30.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--oi-limit", type=int, default=1500)
    parser.add_argument("--lookback-bars", type=int, default=48)
    parser.add_argument("--bins", type=int, default=24)
    parser.add_argument("--value-area-fraction", type=float, default=0.70)
    parser.add_argument("--low-volume-quantile", type=float, default=0.25)
    parser.add_argument("--min-oi-change-pct", type=float, default=0.10)
    parser.add_argument("--oi-lookback-bars", type=int, default=12)
    parser.add_argument("--max-oi-age-bars", type=float, default=6.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    candle_ms = interval_millis(args.interval)
    oi_ms = interval_millis(args.oi_interval)
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or not 2 <= args.oi_limit <= 1500
            or args.lookback_bars < 2 or args.bins < 2 or not 0 < args.value_area_fraction <= 1
            or not 0 <= args.low_volume_quantile <= 1 or args.min_oi_change_pct < 0
            or args.oi_lookback_bars <= 0 or args.max_oi_age_bars < 0
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0
            or candle_ms is None or oi_ms is None):
        parser.error("invalid interval, profile, OI, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    oi_payload = fetch(args.base_url, "/v1/history/open-interest", {
        "exchange": args.oi_exchange, "symbol": args.symbol, "start_ms": start_ms,
        "end_ms": end_ms, "interval": args.oi_interval, "limit": args.oi_limit,
    }, args.timeout)
    rows = candle_rows(candle_payload)
    oi = oi_points(oi_payload)
    observations = build_observations(
        rows, oi, args.lookback_bars, args.bins, args.value_area_fraction,
        args.low_volume_quantile, args.min_oi_change_pct,
        args.oi_lookback_bars * oi_ms, args.max_oi_age_bars * candle_ms,
        args.horizon_bars,
    )
    coverage = {"candles": candle_payload.get("coverage_detail"),
                "open_interest": oi_payload.get("coverage_detail")}
    errors = []
    for source, payload in (("candles", candle_payload), ("open_interest", oi_payload)):
        if payload.get("error"):
            errors.append({"source": source, "error": payload["error"]})
    evidence = ["profile_vwap_oi_and_forward_returns_available" if observations
                else "no_aligned_profile_vwap_oi_windows"]
    for source, detail in coverage.items():
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{source}_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_profile_vwap_oi_response_replay",
        "hypothesis": "price outside a prior value area on the same side of VWAP with fresh matching OI change may differ from non-confluence controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval,
                   "oi_exchange": args.oi_exchange, "oi_interval": args.oi_interval},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"lookback_bars": args.lookback_bars, "bins": args.bins,
                    "value_area_fraction": args.value_area_fraction,
                    "low_volume_quantile": args.low_volume_quantile,
                    "min_oi_change_pct": args.min_oi_change_pct,
                    "oi_lookback_bars": args.oi_lookback_bars,
                    "max_oi_age_bars": args.max_oi_age_bars,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"candles": len(rows), "open_interest": len(oi),
                           "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": errors,
        "limitations": [
            "profile assigns each candle's full volume to one typical-price bin, not tick-level volume-at-price",
            "VWAP is a trailing lookback proxy, not an exchange execution benchmark or universal session VWAP",
            "aggregate OI has provider units and does not reveal position ownership, intent or liquidation",
            "profile, VWAP and OI thresholds plus overlapping horizons are sensitivity choices",
            "missing bars, costs, funding, borrow, latency, queue, slippage and fills are not inferred",
            "no order, wallet, allocation or live-execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
