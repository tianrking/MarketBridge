#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time Money Flow Index states.

MFI combines typical price and candle volume.  This implementation keeps the
volume convention explicit, labels overbought/oversold/neutral states and
threshold reclaims, then measures a fixed future response.  It is a volume-
weighted oscillator study, not a money-flow ownership, forecast or execution
model.
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


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        values = {key: number(row.get(key))
                  for key in ("open", "high", "low", "close", "volume")}
        timestamp = row.get("open_time_ms")
        if (isinstance(timestamp, int) and all(value is not None for value in values.values())
                and values["open"] > 0 and values["close"] > 0
                and values["high"] >= values["low"] > 0 and values["volume"] >= 0
                and values["high"] >= values["open"] and values["high"] >= values["close"]
                and values["low"] <= values["open"] and values["low"] <= values["close"]):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def money_flow_inputs(rows):
    typical_prices = [(row["high"] + row["low"] + row["close"]) / 3.0 for row in rows]
    raw_flows = [price * row["volume"] for price, row in zip(typical_prices, rows)]
    positive = [0.0]
    negative = [0.0]
    for index in range(1, len(rows)):
        if typical_prices[index] > typical_prices[index - 1]:
            positive.append(raw_flows[index])
            negative.append(0.0)
        elif typical_prices[index] < typical_prices[index - 1]:
            positive.append(0.0)
            negative.append(raw_flows[index])
        else:
            positive.append(0.0)
            negative.append(0.0)
    return typical_prices, raw_flows, positive, negative


def mfi_series(rows, period, overbought=80.0, oversold=20.0):
    """Return MFI features after a complete, as-of flow window."""
    if (period <= 0 or not 0 <= oversold < overbought <= 100):
        return []
    series = [None] * len(rows)
    if len(rows) < period + 1:
        return series
    typical, raw, positive, negative = money_flow_inputs(rows)
    for index in range(period, len(rows)):
        positive_sum = sum(positive[index - period + 1:index + 1])
        negative_sum = sum(negative[index - period + 1:index + 1])
        if negative_sum <= 0:
            mfi = 100.0 if positive_sum > 0 else 50.0
        else:
            ratio = positive_sum / negative_sum
            mfi = 100.0 - 100.0 / (1.0 + ratio)
        if mfi >= overbought:
            state, direction = "overbought", -1
        elif mfi <= oversold:
            state, direction = "oversold", 1
        else:
            state, direction = "neutral", 0
        prior = series[index - 1]
        event = None
        if prior is not None:
            if prior["mfi"] <= oversold < mfi:
                event = "oversold_reclaim"
            elif prior["mfi"] >= overbought > mfi:
                event = "overbought_rejection"
        event_direction = (1 if event == "oversold_reclaim"
                           else -1 if event == "overbought_rejection" else None)
        series[index] = {
            "typical_price": typical[index],
            "raw_money_flow": raw[index],
            "positive_money_flow": positive_sum,
            "negative_money_flow": negative_sum,
            "mfi": mfi,
            "overbought": overbought,
            "oversold": oversold,
            "direction_sign": direction,
            "event_direction_sign": event_direction,
            "state": state,
            "event": event,
        }
    return series


def mfi_features(rows, index, period, overbought=80.0, oversold=20.0):
    series = mfi_series(rows, period, overbought, oversold)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, period, overbought, oversold, horizon_bars):
    if (period <= 0 or horizon_bars <= 0 or not 0 <= oversold < overbought <= 100):
        raise ValueError("invalid MFI period, thresholds or horizon")
    series = mfi_series(rows, period, overbought, oversold)
    observations = []
    for index in range(period, len(rows) - horizon_bars):
        features = series[index]
        if features is None:
            continue
        future = rows[index + horizon_bars]
        path = rows[index + 1:index + horizon_bars + 1]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = (features["event_direction_sign"]
                     if features["event_direction_sign"] is not None
                     else features["direction_sign"])
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "state": features["state"],
            "event": features["event"],
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
    states = ("overbought", "oversold", "neutral")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("oversold_reclaim", "overbought_rejection")
    }
    extreme = by_state["overbought"]["observations"] + by_state["oversold"]["observations"]
    control = by_state["neutral"]["observations"]
    sufficient = extreme >= min_observations and control >= min_observations
    return {
        "observations": len(observations),
        "extreme_observations": extreme,
        "neutral_control_observations": control,
        "threshold_event_observations": sum(bucket["observations"] for bucket in events.values()),
        "by_state": by_state,
        "by_event": events,
        "verdict": ("mfi_response_reported" if sufficient
                     else "observe_only_insufficient_extreme_or_neutral_control"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=180.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--period", type=int, default=14)
    parser.add_argument("--overbought", type=float, default=80.0)
    parser.add_argument("--oversold", type=float, default=20.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.period <= 0
            or not 0 <= args.oversold < args.overbought <= 100 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid period, thresholds, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.period, args.overbought, args.oversold,
                                      args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_mfi_response_replay",
        "hypothesis": "point-in-time volume-weighted money-flow extremes and threshold reclaims may have different later responses from neutral controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"period": args.period, "overbought": args.overbought,
                    "oversold": args.oversold, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "MFI uses typical price times candle volume; it is not aggressive flow, exchange flow or holder ownership",
            "overbought/oversold thresholds, period and row-count horizon are caller-supplied sensitivity parameters",
            "extreme and reclaim labels are descriptive associations, not reversal guarantees or execution rules",
            "missing volume is excluded by candle validation rather than zero-filled",
            "overlapping windows do not establish causality; fees, funding, slippage, queue and fills are omitted",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
