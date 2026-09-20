#!/usr/bin/env python3
"""Rank Binance perpetual short-reversal research candidates.

The scanner consumes MarketBridge's read-only squeeze scan and completed
perpetual candles.  It reuses the conservative reversal state machine, then
derives transparent reference levels from the latest confirmed structure and
ATR.  These are mechanical research levels, not orders, forecasts, or
personalized investment advice.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_short_squeeze_reversal_monitor import (
    candle_rows,
    classify,
    latest_state,
    price_structure,
)


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        result = float(value)
    except (TypeError, ValueError):
        return None
    return result if result == result and result > 0 else None


def average_true_range(rows, period=14):
    """Return a simple completed-bar ATR; None means insufficient history."""
    if period <= 0 or len(rows) < period + 1:
        return None
    true_ranges = []
    for previous, current in zip(rows[-period - 1:-1], rows[-period:]):
        true_ranges.append(max(
            current["high"] - current["low"],
            abs(current["high"] - previous["close"]),
            abs(current["low"] - previous["close"]),
        ))
    value = sum(true_ranges) / len(true_ranges)
    return value if value > 0 else None


def short_research_levels(rows, structure, atr_period=14, lookback_bars=24,
                          entry_buffer_atr=0.15, stop_buffer_atr=0.25):
    """Derive auditable reference levels from a confirmed lower-high/lower-low.

    The reference entry is the failed-breakdown level (the previous bar low),
    not a promise of a fill.  The invalidation is above the failed retest and
    the observed impulse high.  Targets are fixed 1R/2R distances and a
    recent-swing support reference, so callers can evaluate them later.
    """
    if not structure.get("confirmed") or len(rows) < atr_period + 2:
        return {"available": False, "reason": "reversal_structure_or_atr_unavailable"}
    atr = average_true_range(rows, atr_period)
    impulse_high = number(structure.get("impulse_high"))
    previous_low = number(structure.get("previous_low"))
    previous_high = number(structure.get("previous_high"))
    latest_close = number(rows[-1].get("close"))
    if not all(value is not None for value in (atr, impulse_high, previous_low,
                                               previous_high, latest_close)):
        return {"available": False, "reason": "required_structure_price_missing"}
    entry = previous_low
    stop = max(previous_high, impulse_high) + stop_buffer_atr * atr
    risk = stop - entry
    if risk <= 0:
        return {"available": False, "reason": "non_positive_reference_risk"}
    support_rows = rows[max(0, len(rows) - lookback_bars):-1]
    support = min((row["low"] for row in support_rows), default=None)
    return {
        "available": True,
        "method": "failed_breakdown_reference_atr_r_levels_v1",
        "reference_entry": entry,
        "entry_zone": [entry - entry_buffer_atr * atr, entry + entry_buffer_atr * atr],
        "invalidation": stop,
        "risk_per_unit": risk,
        "target_1r": entry - risk,
        "target_2r": entry - 2 * risk,
        "recent_swing_support": support,
        "latest_close": latest_close,
        "atr": atr,
        "atr_period": atr_period,
        "completed_bar_ts_ms": rows[-1].get("ts_ms"),
        "execution": "research_only_no_orders",
    }


def scan_payload(payload, candle_payloads, args, as_of_ms):
    candidates = []
    for candidate in payload.get("candidates", []) if isinstance(payload, dict) else []:
        if str(candidate.get("exchange", "")).lower() != "binance":
            continue
        symbol = str(candidate.get("symbol", "")).upper()
        rows = candle_rows(candle_payloads.get(symbol, {}))
        raw = candidate.get("raw") or {}
        decision = classify(raw, price_structure(rows, args.lookback_bars),
                            args.min_fuel_score, args.min_oi_drop_pct,
                            args.funding_normalized_abs,
                            args.min_liquidation_notional,
                            args.min_liquidation_volume_ratio)
        structure = price_structure(rows, args.lookback_bars)
        levels = short_research_levels(rows, structure, args.atr_period, args.lookback_bars)
        if decision["verdict"] != "reversal_confirmed_research_candidate":
            continue
        candidates.append({
            "symbol": symbol,
            "exchange": "binance",
            "verdict": decision["verdict"],
            "score": candidate.get("score"),
            "evidence": decision.get("evidence", []),
            "inputs": decision.get("inputs", {}),
            "price_structure": structure,
            "levels": levels,
            "source_data_quality": candidate.get("data_quality"),
            "execution": "research_only_no_orders",
        })
    return {
        "event": "marketbridge_binance_short_opportunity_scan",
        "as_of_ms": as_of_ms,
        "exchange": "binance",
        "candidates": candidates,
        "observed_candidates": len(payload.get("candidates", [])) if isinstance(payload, dict) else 0,
        "limitations": [
            "Levels are completed-bar reference levels, not guaranteed fills or price forecasts.",
            "The provider liquidation side remains a directional proxy and is not universal across venues.",
            "A high-probability claim requires archived outcomes and walk-forward calibration.",
            "No liquidation heatmap or hidden liquidation-wall target is inferred.",
            "No order, wallet signing, borrowing, allocation, or execution path exists.",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--candle-interval", default="5m")
    parser.add_argument("--candle-limit", type=int, default=120)
    parser.add_argument("--lookback-bars", type=int, default=24)
    parser.add_argument("--atr-period", type=int, default=14)
    parser.add_argument("--max-data-age-ms", type=int, default=15000)
    parser.add_argument("--minimum-score", type=int, default=5)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--min-fuel-score", type=int, default=6)
    parser.add_argument("--min-oi-drop-pct", type=float, default=3.0)
    parser.add_argument("--funding-normalized-abs", type=float, default=0.0002)
    parser.add_argument("--min-liquidation-notional", type=float, default=0.0)
    parser.add_argument("--min-liquidation-volume-ratio", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.candle_limit < args.atr_period + 2 or args.lookback_bars < 2
            or args.atr_period <= 0 or args.limit <= 0 or args.timeout <= 0
            or args.minimum_score < 0 or args.max_data_age_ms <= 0
            or args.min_fuel_score < 0 or args.min_oi_drop_pct < 0
            or args.funding_normalized_abs < 0 or args.min_liquidation_notional < 0
            or args.min_liquidation_volume_ratio < 0):
        parser.error("invalid scan, candle, threshold or timeout arguments")
    now_ms = int(time.time() * 1000)
    payload = fetch(args.base_url, "/v1/research/squeeze/scan", {
        "exchange": "binance", "max_data_age_ms": args.max_data_age_ms,
        "minimum_score": args.minimum_score, "limit": args.limit,
    }, args.timeout)
    candle_payloads = {}
    for candidate in payload.get("candidates", []):
        symbol = str(candidate.get("symbol", "")).upper()
        if symbol:
            candle_payloads[symbol] = fetch(args.base_url, "/v1/history/candles", {
                "exchange": "binance", "market": "perp", "symbol": symbol,
                "interval": args.candle_interval, "limit": args.candle_limit,
                "end_ms": now_ms - 60_000,
            }, args.timeout)
    print(json.dumps(scan_payload(payload, candle_payloads, args, now_ms),
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
