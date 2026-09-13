#!/usr/bin/env python3
"""Replay a crypto volatility-compression breakout hypothesis.

The hypothesis is deliberately narrow: a close that breaks a recent range
after a low-realized-volatility regime may continue for a fixed candle horizon,
especially when candle volume and (optionally) historical taker flow confirm the
break. This is a bounded descriptive replay, not a trading or execution engine.
"""

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
    return float(value) if isinstance(value, (int, float)) else None


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        high = number(row.get("high"))
        low = number(row.get("low"))
        if isinstance(ts_ms, int) and close is not None and close > 0 and high is not None and low is not None:
            rows.append({
                "ts_ms": ts_ms,
                "open": number(row.get("open")),
                "high": high,
                "low": low,
                "close": close,
                "volume": number(row.get("volume")),
            })
    return sorted(rows, key=lambda row: row["ts_ms"])


def realized_vol_pct(closes):
    """Population volatility of log returns, expressed in percent per window."""
    if len(closes) < 2 or any(value is None or value <= 0 for value in closes):
        return None
    returns = [math.log(current / previous) for previous, current in zip(closes, closes[1:])]
    return statistics.pstdev(returns) * 100.0


def volatility_regime(closes, index, compression_window, baseline_window):
    compression_start = index - compression_window
    baseline_end = compression_start
    baseline_start = baseline_end - baseline_window
    if baseline_start < 1:
        return {"compression_pct": None, "baseline_pct": None, "ratio": None}
    compression = realized_vol_pct(closes[compression_start:index])
    baseline = realized_vol_pct(closes[baseline_start:baseline_end])
    ratio = compression / baseline if compression is not None and baseline and baseline > 0 else None
    return {"compression_pct": compression, "baseline_pct": baseline, "ratio": ratio}


def breakout_features(rows, index, range_bars, compression_window, baseline_window, max_compression_ratio, breakout_buffer, volume_multiplier):
    if index < max(range_bars, compression_window + baseline_window + 1):
        return None
    close = rows[index]["close"]
    prior = rows[index - range_bars:index]
    prior_high = max(row["high"] for row in prior)
    prior_low = min(row["low"] for row in prior)
    regime = volatility_regime([row["close"] for row in rows], index, compression_window, baseline_window)
    compressed = regime["ratio"] is not None and regime["ratio"] <= max_compression_ratio
    direction = 1 if close > prior_high * (1.0 + breakout_buffer) else -1 if close < prior_low * (1.0 - breakout_buffer) else 0
    previous_volumes = [row["volume"] for row in prior if row["volume"] is not None and row["volume"] >= 0]
    volume = rows[index]["volume"]
    average_volume = statistics.mean(previous_volumes) if previous_volumes else None
    volume_ratio = volume / average_volume if volume is not None and average_volume and average_volume > 0 else None
    volume_confirmed = volume_ratio is not None and volume_ratio >= volume_multiplier
    return {
        "direction": direction,
        "prior_high": prior_high,
        "prior_low": prior_low,
        "close": close,
        "regime": regime,
        "compressed": compressed,
        "volume": volume,
        "average_volume": average_volume,
        "volume_ratio": volume_ratio,
        "volume_confirmed": volume_confirmed,
    }


def forward_return(rows, index, horizon_bars):
    target = index + horizon_bars
    if target >= len(rows) or rows[index]["close"] <= 0:
        return None
    return (rows[target]["close"] / rows[index]["close"] - 1.0) * 100.0


def flow_ratio_at(rows, start_ms, window_ms):
    selected = [row for row in rows if start_ms <= row.get("ts_ms", -1) <= start_ms + window_ms]
    buy = sum(number(row.get("notional")) or 0.0 for row in selected if str(row.get("side", "")).lower() == "buy")
    sell = sum(number(row.get("notional")) or 0.0 for row in selected if str(row.get("side", "")).lower() == "sell")
    total = buy + sell
    return {
        "ratio": (buy - sell) / total if total > 0 else None,
        "buy_notional": buy if total > 0 else None,
        "sell_notional": sell if total > 0 else None,
        "trade_count": len(selected),
    }


def classify_event(features, flow, flow_threshold):
    if features is None or features["direction"] == 0:
        return "no_breakout"
    if not features["compressed"]:
        return "breakout_without_compression"
    direction = features["direction"]
    if flow is None or flow.get("ratio") is None:
        return "breakout_volume_confirmed_missing_flow" if features["volume_confirmed"] else "breakout_missing_flow_or_weak_volume"
    flow_direction = 1 if flow["ratio"] >= flow_threshold else -1 if flow["ratio"] <= -flow_threshold else 0
    if flow_direction and flow_direction != direction:
        return "breakout_flow_conflict"
    if flow_direction == direction and features["volume_confirmed"]:
        return "breakout_confirmed_by_volume_and_flow"
    if flow_direction == direction:
        return "breakout_flow_confirmed_weak_volume"
    return "breakout_weak_flow"


def summarize(events):
    valid = [event for event in events if event["forward_return_pct"] is not None]
    aligned = [event["direction_sign"] * event["forward_return_pct"] for event in valid]
    return {
        "events": len(events),
        "forward_observations": len(valid),
        "mean_aligned_return_pct": statistics.mean(aligned) if aligned else None,
        "median_aligned_return_pct": statistics.median(aligned) if aligned else None,
        "directional_hit_rate": (sum(value > 0 for value in aligned) / len(aligned)) if aligned else None,
        "by_class": {
            state: sum(event["classification"] == state for event in events)
            for state in sorted({event["classification"] for event in events})
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--days", type=float, default=3.0)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--range-bars", type=int, default=12)
    parser.add_argument("--compression-window", type=int, default=12)
    parser.add_argument("--baseline-window", type=int, default=48)
    parser.add_argument("--max-compression-ratio", type=float, default=0.75)
    parser.add_argument("--breakout-buffer", type=float, default=0.0)
    parser.add_argument("--volume-multiplier", type=float, default=1.20)
    parser.add_argument("--horizon-bars", type=int, default=6)
    parser.add_argument("--flow-exchange", choices=("none", "binance", "okx"), default="none")
    parser.add_argument("--flow-pages", type=int, default=12,
                        help="historical trade pages/windows requested from MarketBridge")
    parser.add_argument("--flow-window-ms", type=int, default=300_000)
    parser.add_argument("--flow-threshold", type=float, default=0.20)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.days <= 0 or not 0 < options.limit <= 1500 or options.range_bars <= 0
            or options.compression_window <= 1 or options.baseline_window <= 1
            or not 0 < options.max_compression_ratio < 2 or options.breakout_buffer < 0
            or options.volume_multiplier < 0 or options.horizon_bars <= 0
            or not 1 <= options.flow_pages <= 48 or options.flow_window_ms <= 0
            or not 0 < options.flow_threshold < 1):
        parser.error("invalid replay windows, thresholds or limits")

    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(options.days * 86_400_000)
    candle_payload = fetch(options.base_url, "/v1/history/candles", {
        "exchange": options.exchange, "symbol": options.symbol, "market": options.market,
        "interval": options.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": options.limit,
    }, options.timeout)
    rows = candle_rows(candle_payload)
    flow_rows = None
    flow_error = None
    flow_coverage = None
    if options.flow_exchange != "none":
        try:
            flow_payload = fetch(options.base_url, "/v1/history/trades", {
                "exchange": options.flow_exchange, "symbol": options.symbol,
                "start_ms": start_ms, "end_ms": end_ms, "limit": 1000,
                "pages": options.flow_pages,
            }, options.timeout)
            flow_rows = flow_payload.get("rows", [])
            flow_error = flow_payload.get("error")
            flow_coverage = flow_payload.get("coverage_detail")
        except Exception as error:  # network/provider gaps remain evidence, not a zero
            flow_error = str(error)

    events = []
    closes = [row["close"] for row in rows]
    minimum_index = max(options.range_bars, options.compression_window + options.baseline_window + 1)
    for index in range(minimum_index, max(minimum_index, len(rows) - options.horizon_bars)):
        features = breakout_features(rows, index, options.range_bars, options.compression_window,
                                     options.baseline_window, options.max_compression_ratio,
                                     options.breakout_buffer, options.volume_multiplier)
        if features is None or features["direction"] == 0 or not features["compressed"]:
            continue
        flow = flow_ratio_at(flow_rows, rows[index]["ts_ms"], options.flow_window_ms) if flow_rows is not None else None
        events.append({
            "ts_ms": rows[index]["ts_ms"],
            "direction": "up" if features["direction"] > 0 else "down",
            "direction_sign": features["direction"],
            "features": features,
            "flow": flow,
            "classification": classify_event(features, flow, options.flow_threshold),
            "forward_return_pct": forward_return(rows, index, options.horizon_bars),
        })

    evidence = ["historical_candles_available" if rows else "missing_historical_candles"]
    evidence.append("historical_taker_flow_available" if flow_rows is not None and flow_rows else
                    "taker_flow_not_requested" if options.flow_exchange == "none" else "missing_historical_taker_flow")
    if flow_coverage:
        coverage_status = flow_coverage.get("status")
        evidence.append(f"taker_flow_coverage_{coverage_status}" if coverage_status else "taker_flow_coverage_reported")
    if flow_error:
        evidence.append("taker_flow_provider_error")
    print(json.dumps({
        "strategy": "crypto_volatility_breakout_replay",
        "market": {"exchange": options.exchange, "symbol": options.symbol,
                   "market": options.market, "interval": options.interval},
        "parameters": {"days": options.days, "limit": options.limit, "range_bars": options.range_bars,
                       "compression_window": options.compression_window, "baseline_window": options.baseline_window,
                       "max_compression_ratio": options.max_compression_ratio, "breakout_buffer": options.breakout_buffer,
                       "volume_multiplier": options.volume_multiplier, "horizon_bars": options.horizon_bars,
                       "flow_exchange": options.flow_exchange, "flow_pages": options.flow_pages,
                       "flow_window_ms": options.flow_window_ms,
                       "flow_threshold": options.flow_threshold},
        "bars_available": len(rows),
        "summary": summarize(events),
        "events": events,
        "flow_coverage": flow_coverage,
        "evidence": evidence,
        "provider_error": flow_error or candle_payload.get("error"),
        "execution": "research_only_no_orders",
        "limitations": [
            "compression and breakout thresholds are research parameters, not universal constants",
            "forward return is close-to-close and excludes fees, spread, slippage, funding and latency",
            "historical trade endpoints are bounded; inspect flow_coverage before treating flow as complete",
            "public trade history does not reconstruct every private or block execution",
            "a missing volume or flow confirmation is an evidence gap, not a zero",
        ],
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
