#!/usr/bin/env python3
"""Replay price/OBV divergence responses from historical OHLCV candles.

OBV adds a candle's full volume when its close is above the previous close and
subtracts it when the close is below.  This replay normalizes the OBV change by
the window's total volume, classifies price/OBV agreement or divergence, and
measures later fixed-horizon responses.  It is a descriptive volume-flow study,
not a trader-intent, accumulation, or execution model.
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
        values = {key: number(row.get(key)) for key in ("open", "high", "low", "close", "volume")}
        timestamp = row.get("open_time_ms")
        if (isinstance(timestamp, int) and all(value is not None for value in values.values())
                and values["open"] > 0 and values["close"] > 0
                and values["high"] >= values["low"] > 0
                and values["volume"] >= 0
                and values["high"] >= values["open"]
                and values["high"] >= values["close"]
                and values["low"] <= values["open"]
                and values["low"] <= values["close"]):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def obv_values(rows):
    """Return an as-of OBV series; missing volume keeps later values missing."""
    if not rows:
        return []
    values = [0.0]
    for previous, current in zip(rows, rows[1:]):
        if values[-1] is None or current.get("volume") is None or current["volume"] < 0:
            values.append(None)
        elif current["close"] > previous["close"]:
            values.append(values[-1] + current["volume"])
        elif current["close"] < previous["close"]:
            values.append(values[-1] - current["volume"])
        else:
            values.append(values[-1])
    return values


def classify_state(price_return_bps, obv_flow_fraction, price_threshold_bps,
                   obv_threshold):
    if price_return_bps is None or obv_flow_fraction is None:
        return "missing_volume", 0
    price_up = price_return_bps >= price_threshold_bps
    price_down = price_return_bps <= -price_threshold_bps
    obv_up = obv_flow_fraction >= obv_threshold
    obv_down = obv_flow_fraction <= -obv_threshold
    if price_up and obv_down:
        return "bearish_divergence", -1
    if price_down and obv_up:
        return "bullish_divergence", 1
    if price_up and obv_up:
        return "price_up_obv_up", 1
    if price_down and obv_down:
        return "price_down_obv_down", -1
    return "flat_or_mixed", 0


def observation_at(rows, obv, index, lookback_bars, price_threshold_bps, obv_threshold):
    if index < lookback_bars or index >= len(rows) or len(obv) != len(rows):
        return None
    start = index - lookback_bars
    if obv[index] is None or obv[start] is None:
        return None
    volume_window = [row["volume"] for row in rows[start + 1:index + 1]
                     if row.get("volume") is not None and row["volume"] >= 0]
    total_volume = sum(volume_window)
    if total_volume <= 0:
        return None
    price_return_bps = (rows[index]["close"] / rows[start]["close"] - 1.0) * 10_000.0
    obv_flow_fraction = (obv[index] - obv[start]) / total_volume
    state, direction = classify_state(
        price_return_bps, obv_flow_fraction, price_threshold_bps, obv_threshold,
    )
    return {
        "ts_ms": rows[index]["ts_ms"],
        "lookback_start_ts_ms": rows[start]["ts_ms"],
        "price_return_bps": price_return_bps,
        "obv_flow_fraction": obv_flow_fraction,
        "state": state,
        "direction_sign": direction,
    }


def build_observations(rows, lookback_bars, horizon_bars, price_threshold_bps,
                       obv_threshold):
    if lookback_bars <= 0 or horizon_bars <= 0 or price_threshold_bps < 0 or not 0 <= obv_threshold <= 1:
        raise ValueError("invalid lookback, horizon or OBV thresholds")
    obv = obv_values(rows)
    observations = []
    for index in range(lookback_bars, len(rows) - horizon_bars):
        state = observation_at(rows, obv, index, lookback_bars,
                               price_threshold_bps, obv_threshold)
        if state is None:
            continue
        future = rows[index + horizon_bars]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = state["direction_sign"]
        observations.append({
            **state,
            "future_ts_ms": future["ts_ms"],
            "forward_return_pct": forward,
            "direction_aligned_return_bps": direction * forward * 100.0 if direction else None,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (
                min(row["low"] for row in rows[index + 1:index + horizon_bars + 1])
                / rows[index]["close"] - 1.0
            ) * 100.0,
            "forward_max_path_return_pct": (
                max(row["high"] for row in rows[index + 1:index + horizon_bars + 1])
                / rows[index]["close"] - 1.0
            ) * 100.0,
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
    states = ("bullish_divergence", "bearish_divergence", "price_up_obv_up",
              "price_down_obv_down", "flat_or_mixed", "missing_volume")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    divergence_count = sum(row["state"].endswith("divergence") for row in observations)
    control_count = sum(row["state"] in {"price_up_obv_up", "price_down_obv_down", "flat_or_mixed"}
                        for row in observations)
    sufficient = divergence_count >= min_observations and control_count >= min_observations
    return {
        "observations": len(observations),
        "divergence_signals": divergence_count,
        "control_observations": control_count,
        "by_state": by_state,
        "verdict": ("obv_divergence_response_reported" if sufficient
                     else "observe_only_insufficient_obv_or_control"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="4h")
    parser.add_argument("--days", type=float, default=730.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--lookback-bars", type=int, default=20)
    parser.add_argument("--price-threshold-bps", type=float, default=20.0)
    parser.add_argument("--obv-threshold", type=float, default=0.10)
    parser.add_argument("--horizon-bars", type=int, default=6)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.lookback_bars <= 0
            or args.price_threshold_bps < 0 or not 0 <= args.obv_threshold <= 1
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid lookback, thresholds, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.lookback_bars, args.horizon_bars,
        args.price_threshold_bps, args.obv_threshold,
    )
    summary = summarize(observations, args.min_observations)
    print(json.dumps({
        "strategy": "crypto_obv_divergence_response_replay",
        "hypothesis": "price and close-signed volume-flow divergence may have a different later response from agreement and mixed controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"lookback_bars": args.lookback_bars,
                    "price_threshold_bps": args.price_threshold_bps,
                    "obv_threshold": args.obv_threshold,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations),
                          "divergence_signals": summary["divergence_signals"]},
        "observations": observations,
        "summary": summary,
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "OBV assigns an entire candle volume to close direction and is not aggressive buy/sell flow or trader ownership",
            "the normalized OBV flow threshold, price threshold, lookback and horizon are caller-supplied sensitivity parameters",
            "missing volume is retained as missing_volume rather than zero-filled",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "divergence and response are descriptive associations; overlapping windows do not establish causality",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
