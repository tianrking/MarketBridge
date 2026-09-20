#!/usr/bin/env python3
"""Monitor a short-squeeze exhaustion -> reversal research candidate.

This is deliberately a paper/research monitor, not a short instruction.  It
requires evidence that a short-squeeze impulse has already occurred, then
waits for OI deleveraging, funding normalization and a lower-high/lower-low
price structure before reporting a candidate.  A still-rising squeeze is
reported as ``squeeze_active_no_short``.
"""

import argparse
import json
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
        result = float(value)
    except (TypeError, ValueError):
        return None
    return result if result == result else None


def latest_state(payload, symbol, exchange):
    rows = payload.get("states", []) if isinstance(payload, dict) else []
    for row in rows:
        if (str(row.get("symbol", "")).upper() == symbol.upper()
                and str(row.get("exchange", "")).lower() == exchange.lower()):
            return row
    return rows[0] if len(rows) == 1 and isinstance(rows[0], dict) else None


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []) if isinstance(payload, dict) else []:
        ts_ms = row.get("open_time_ms")
        values = {key: number(row.get(key)) for key in ("open", "high", "low", "close")}
        if (isinstance(ts_ms, int) and all(value is not None and value > 0 for value in values.values())
                and values["high"] >= values["low"]):
            rows.append({"ts_ms": ts_ms, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def price_structure(rows, lookback_bars=24):
    """Classify only a completed-bar lower-high/lower-low confirmation."""
    if len(rows) < 3:
        return {"state": "insufficient_candles", "confirmed": False}
    previous, latest = rows[-2], rows[-1]
    impulse = rows[max(0, len(rows) - 2 - lookback_bars):-2]
    impulse_high = max((row["high"] for row in impulse), default=None)
    lower_high = latest["high"] < previous["high"]
    lower_low = latest["low"] < previous["low"]
    close_below_previous = latest["close"] < previous["close"]
    failed_retest = impulse_high is not None and previous["high"] >= impulse_high
    confirmed = lower_high and lower_low and close_below_previous and failed_retest
    return {
        "state": "lower_high_lower_low" if confirmed else "no_confirmed_reversal",
        "confirmed": confirmed,
        "latest_ts_ms": latest["ts_ms"],
        "previous_high": previous["high"],
        "latest_high": latest["high"],
        "previous_low": previous["low"],
        "latest_low": latest["low"],
        "impulse_high": impulse_high,
        "lower_high": lower_high,
        "lower_low": lower_low,
        "close_below_previous": close_below_previous,
        "failed_retest": failed_retest,
    }


def rolling_change(metrics, window_ms):
    for row in metrics.get("open_interest_changes", []) or []:
        if row.get("window_ms") == window_ms:
            return number(row.get("change_pct"))
    return None


def classify(state, structure, min_fuel_score=6, min_oi_drop_pct=3.0,
             funding_normalized_abs=0.0002, min_liquidation_notional=0.0):
    """Return a conservative state machine decision from one as-of snapshot."""
    state = state or {}
    metrics = state.get("metrics") or {}
    long_squeeze = state.get("long_squeeze") or {}
    score = number(long_squeeze.get("score"))
    buy_liq = number(metrics.get("buy_liquidation_notional_15m"))
    funding = number(metrics.get("funding_rate"))
    oi_15m = rolling_change(metrics, 900_000)
    oi_1h = rolling_change(metrics, 3_600_000)
    price_15m = next((number(row.get("change_pct")) for row in metrics.get("price_changes", []) or []
                      if row.get("window_ms") == 900_000), None)
    fuel = ((score is not None and score >= min_fuel_score)
            and buy_liq is not None and buy_liq >= min_liquidation_notional)
    active = long_squeeze.get("state") == "triggered_long_squeeze"
    deleveraging = ((oi_15m is not None and oi_15m <= -min_oi_drop_pct)
                    or (oi_1h is not None and oi_1h <= -min_oi_drop_pct))
    funding_normalized = funding is not None and abs(funding) <= funding_normalized_abs
    reversal = bool(structure.get("confirmed")) and price_15m is not None and price_15m < 0
    evidence = []
    if fuel:
        evidence.append("recent_provider_buy_side_liquidation_and_squeeze_score")
    if active:
        evidence.append("long_squeeze_state_still_triggered")
    if deleveraging:
        evidence.append("open_interest_deleveraging")
    if funding_normalized:
        evidence.append("funding_normalized")
    if reversal:
        evidence.append("lower_high_lower_low_with_negative_15m_response")
    if active:
        verdict = "squeeze_active_no_short"
    elif fuel and deleveraging and funding_normalized and reversal:
        verdict = "reversal_confirmed_research_candidate"
    elif fuel and deleveraging:
        verdict = "squeeze_exhaustion_watch"
    else:
        verdict = "observe_only"
    return {
        "verdict": verdict,
        "research_only": True,
        "fuel_observed": fuel,
        "deleveraging_observed": deleveraging,
        "funding_normalized": funding_normalized,
        "reversal_confirmed": reversal,
        "evidence": evidence,
        "inputs": {"squeeze_score": score, "buy_liquidation_notional_15m": buy_liq,
                   "funding_rate": funding, "oi_change_15m_pct": oi_15m,
                   "oi_change_1h_pct": oi_1h, "price_change_15m_pct": price_15m},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="FILUSDT")
    parser.add_argument("--candle-interval", default="5m")
    parser.add_argument("--candle-limit", type=int, default=120)
    parser.add_argument("--lookback-bars", type=int, default=24)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--min-fuel-score", type=int, default=6)
    parser.add_argument("--min-oi-drop-pct", type=float, default=3.0)
    parser.add_argument("--funding-normalized-abs", type=float, default=0.0002)
    parser.add_argument("--min-liquidation-notional", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.candle_limit < 3 or args.candle_limit > 1500 or args.lookback_bars <= 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.min_fuel_score < 0
            or args.min_oi_drop_pct < 0 or args.funding_normalized_abs < 0
            or args.min_liquidation_notional < 0 or args.timeout <= 0):
        parser.error("invalid candle, threshold, iteration or timeout arguments")
    for iteration in range(args.iterations):
        now_ms = int(time.time() * 1000)
        state_payload = fetch(args.base_url, "/v1/research/symbol-state", {
            "symbol": args.symbol, "exchange": args.exchange,
        }, args.timeout)
        candle_payload = fetch(args.base_url, "/v1/history/candles", {
            "exchange": args.exchange, "market": "perp", "symbol": args.symbol,
            "interval": args.candle_interval, "limit": args.candle_limit,
            "end_ms": now_ms - 60_000,
        }, args.timeout)
        state = latest_state(state_payload, args.symbol, args.exchange)
        structure = price_structure(candle_rows(candle_payload), args.lookback_bars)
        decision = classify(state, structure, args.min_fuel_score, args.min_oi_drop_pct,
                            args.funding_normalized_abs, args.min_liquidation_notional)
        print(json.dumps({
            "strategy": "crypto_short_squeeze_reversal_monitor",
            "iteration": iteration + 1,
            "as_of_ms": now_ms,
            "market": {"exchange": args.exchange, "symbol": args.symbol,
                       "candle_interval": args.candle_interval},
            "decision": decision,
            "price_structure": structure,
            "source_counts": {"candles": len(candle_rows(candle_payload)),
                              "states": len(state_payload.get("states", []))},
            "coverage": candle_payload.get("coverage_detail"),
            "upstream_errors": [payload.get("error") for payload in (state_payload, candle_payload)
                                if payload.get("error")],
            "limitations": [
                "provider buy-side liquidation is only a directional proxy for short liquidation; venue semantics are not universal",
                "one snapshot cannot prove that a prior squeeze state occurred; archive successive outputs for transition studies",
                "funding normalization is a fixed research threshold, not an interval-normalized z-score",
                "lower-high/lower-low uses completed OHLCV bars and does not establish causality or fill prices",
                "no order, wallet signing, borrowing, allocation or execution path is included",
            ],
            "execution": "research_only_no_orders",
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
