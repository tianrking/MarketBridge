#!/usr/bin/env python3
"""Compare crypto option ATM implied volatility with realized volatility.

This is a read-only volatility-risk-premium observation.  It compares the
selected expiry's ATM mark IV with annualized realized volatility from a
specified MarketBridge perp candle window.  The difference is a research
feature, not a short-volatility instruction or a hedge PnL estimate.
"""

import argparse
import json
import math
import statistics

from crypto_microstructure_monitor import fetch
from crypto_options_skew_monitor import expiry_timestamp, observe


def number(value):
    return float(value) if isinstance(value, (int, float)) else None


def interval_minutes(interval):
    if not isinstance(interval, str) or len(interval) < 2:
        return None
    units = {"m": 1, "h": 60, "d": 1440}
    try:
        return int(interval[:-1]) * units[interval[-1].lower()]
    except (KeyError, ValueError):
        return None


def candle_closes(payload):
    rows = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            rows.append((ts_ms, close))
    return sorted(rows)


def annualized_realized_vol_pct(closes, interval):
    minutes = interval_minutes(interval)
    if minutes is None or len(closes) < 2 or any(value <= 0 for value in closes):
        return None
    returns = [math.log(current / previous) for previous, current in zip(closes, closes[1:])]
    periods_per_year = 365.0 * 24.0 * 60.0 / minutes
    return statistics.pstdev(returns) * math.sqrt(periods_per_year) * 100.0


def classify_vrp(vrp_iv_points, threshold):
    if vrp_iv_points is None:
        return "observe_only_missing_iv_or_rv"
    if vrp_iv_points >= threshold:
        return "implied_volatility_premium"
    if vrp_iv_points <= -threshold:
        return "realized_volatility_above_implied"
    return "implied_and_realized_vol_aligned"


def observe_vrp(base_url, currency, venue, expiry_days, price_exchange, symbol,
                market, interval, rv_bars, atm_band, wing_min, wing_max,
                min_skew_iv, min_term_slope_iv, vrp_threshold, timeout):
    option_observation = observe(base_url, currency, venue, expiry_days, atm_band,
                                 wing_min, wing_max, min_skew_iv,
                                 min_term_slope_iv, timeout)
    candle_payload = fetch(base_url, "/v1/history/candles", {
        "exchange": price_exchange,
        "symbol": symbol,
        "market": market,
        "interval": interval,
        "limit": rv_bars + 1,
    }, timeout)
    candles = candle_closes(candle_payload)
    realized = annualized_realized_vol_pct([close for _, close in candles[-(rv_bars + 1):]], interval)
    target = option_observation.get("target_expiry") or {}
    implied = number(target.get("atm_iv"))
    vrp = implied - realized if implied is not None and realized is not None else None
    return {
        "currency": currency.upper(),
        "venue": venue,
        "target_expiry": target,
        "options": {
            "atm_iv_pct": implied,
            "expiry_time": target.get("expiry_time"),
            "days_to_expiry": target.get("days_to_expiry"),
        },
        "realized_volatility": {
            "exchange": price_exchange,
            "symbol": symbol,
            "market": market,
            "interval": interval,
            "bars_available": len(candles),
            "bars_used": min(len(candles), rv_bars + 1),
            "annualized_rv_pct": realized,
        },
        "vrp": {
            "iv_minus_rv_iv_points": vrp,
            "state": classify_vrp(vrp, vrp_threshold),
        },
        "evidence": option_observation.get("evidence", []) + [
            "realized_volatility_window_available" if realized is not None else "missing_realized_volatility_window",
        ],
        "upstream_errors": option_observation.get("upstream_errors", [])
        + ([candle_payload.get("error")] if candle_payload.get("error") else []),
        "limitations": [
            "option ATM IV and perp realized volatility are not identical maturities",
            "annualized RV depends on interval, window and close-to-close estimator",
            "no vol surface, forward variance, hedge ratio, spread, fee or margin model",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--rv-bars", type=int, default=168)
    parser.add_argument("--atm-band", type=float, default=0.03)
    parser.add_argument("--wing-min", type=float, default=0.85)
    parser.add_argument("--wing-max", type=float, default=1.15)
    parser.add_argument("--min-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-term-slope-iv", type=float, default=3.0)
    parser.add_argument("--vrp-threshold", type=float, default=5.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.expiry_days <= 0 or options.rv_bars <= 1 or interval_minutes(options.interval) is None
            or not 0 < options.atm_band < 0.25 or not 0 < options.wing_min < 1
            or options.wing_max <= 1 or options.wing_min >= options.wing_max
            or options.min_skew_iv < 0 or options.min_term_slope_iv < 0
            or options.vrp_threshold < 0):
        parser.error("invalid expiry, RV window, interval, moneyness or threshold arguments")
    result = observe_vrp(
        options.base_url, options.currency, options.venue, options.expiry_days,
        options.price_exchange, options.symbol, options.market, options.interval,
        options.rv_bars, options.atm_band, options.wing_min, options.wing_max,
        options.min_skew_iv, options.min_term_slope_iv, options.vrp_threshold,
        options.timeout,
    )
    print(json.dumps({"strategy": "crypto_options_vrp_monitor", **result},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
