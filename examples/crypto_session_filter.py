#!/usr/bin/env python3
"""Evaluate a time-window crypto momentum hypothesis from MarketBridge klines.

The hypothesis comes from public Polymarket strategy discussions: a short
pre-US-session window may be useful only when VWAP, EMA(9/21), MACD and volume
agree. This script makes that claim falsifiable on normalized exchange candles;
it is an observer, not an entry engine and never places an order.
"""

import argparse
import json
from datetime import datetime, time, timezone
from zoneinfo import ZoneInfo
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode(params)
    request = Request(f"{base_url.rstrip('/')}/v1/market/klines?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(row, key):
    value = row.get(key)
    return float(value) if isinstance(value, (int, float)) else None


def ema(values, period):
    if len(values) < period:
        return None
    result = sum(values[:period]) / period
    multiplier = 2.0 / (period + 1.0)
    for value in values[period:]:
        result = (value - result) * multiplier + result
    return result


def parse_clock(value):
    try:
        hour, minute = (int(part) for part in value.split(":", 1))
        return time(hour, minute)
    except (TypeError, ValueError):
        raise SystemExit("session times must use HH:MM")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--interval", default="1m")
    parser.add_argument("--limit", type=int, default=60)
    parser.add_argument("--timezone", default="America/New_York")
    parser.add_argument("--session-start", default="09:00")
    parser.add_argument("--session-end", default="09:15")
    parser.add_argument("--volume-multiplier", type=float, default=1.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if options.limit < 21:
        raise SystemExit("--limit must be at least 21 for EMA(21)")
    if options.volume_multiplier < 0:
        raise SystemExit("--volume-multiplier cannot be negative")

    payload = fetch(options.base_url, {
        "exchange": options.exchange,
        "market": options.market,
        "symbol": options.symbol,
        "interval": options.interval,
        "limit": options.limit,
    }, options.timeout)
    bars = [
        row for row in payload.get("klines", [])
        if isinstance(row, dict)
        and number(row, "close") is not None
        and number(row, "high") is not None
        and number(row, "low") is not None
    ]
    bars.sort(key=lambda row: row.get("open_time_ms", 0))
    if len(bars) < 21:
        print(json.dumps({
            "strategy": "crypto_session_filter",
            "market": {"exchange": options.exchange, "market": options.market, "symbol": options.symbol, "interval": options.interval},
            "bars_available": len(bars),
            "bars_required": 21,
            "score": 0,
            "max_score": 5,
            "verdict": "observe only",
            "evidence": ["insufficient_klines"],
            "execution": "research_only_no_orders",
            "limitations": [
                "warm up MarketBridge ingestion before evaluating EMA/VWAP features",
                "missing data is an evidence gap, not a zero signal",
            ],
        }, ensure_ascii=False, indent=2, sort_keys=True))
        return

    closes = [number(row, "close") for row in bars]
    highs = [number(row, "high") for row in bars]
    lows = [number(row, "low") for row in bars]
    volumes = [number(row, "volume") for row in bars]
    valid_volumes = [volume for volume in volumes if volume is not None]
    latest = bars[-1]
    latest_close = closes[-1]
    latest_ema9 = ema(closes, 9)
    latest_ema21 = ema(closes, 21)
    ema9_prev = ema(closes[:-1], 9)
    ema21_prev = ema(closes[:-1], 21)
    macd = latest_ema9 - latest_ema21 if latest_ema9 is not None and latest_ema21 is not None else None
    macd_prev = ema9_prev - ema21_prev if ema9_prev is not None and ema21_prev is not None else None
    total_volume = sum(volume for volume in valid_volumes)
    typical_volume = sum(valid_volumes[:-1]) / len(valid_volumes[:-1]) if len(valid_volumes) > 1 else None
    latest_volume = volumes[-1]
    vwap = (
        sum(((high + low + close) / 3.0) * volume for high, low, close, volume in zip(highs, lows, closes, volumes) if volume is not None)
        / total_volume
        if total_volume > 0 else None
    )

    location = ZoneInfo(options.timezone)
    latest_dt = datetime.fromtimestamp(latest.get("open_time_ms", 0) / 1000.0, tz=timezone.utc).astimezone(location)
    session_start = parse_clock(options.session_start)
    session_end = parse_clock(options.session_end)
    if session_start <= session_end:
        in_session = session_start <= latest_dt.time() < session_end
    else:
        in_session = latest_dt.time() >= session_start or latest_dt.time() < session_end

    evidence = []
    score = 0
    if vwap is not None and latest_close > vwap:
        score += 1
        evidence.append("price_above_vwap")
    if latest_ema9 is not None and latest_ema21 is not None and latest_ema9 > latest_ema21:
        score += 1
        evidence.append("ema9_above_ema21")
    if macd is not None and macd_prev is not None and macd > macd_prev:
        score += 1
        evidence.append("macd_accelerating")
    if latest_volume is not None and typical_volume is not None and latest_volume >= typical_volume * options.volume_multiplier:
        score += 1
        evidence.append("volume_confirmed")
    if in_session:
        score += 1
        evidence.append("inside_session_window")

    print(json.dumps({
        "strategy": "crypto_session_filter",
        "market": {"exchange": options.exchange, "market": options.market, "symbol": options.symbol, "interval": options.interval},
        "as_of": latest_dt.isoformat(),
        "session": {"timezone": options.timezone, "start": options.session_start, "end": options.session_end, "active": in_session},
        "features": {"close": latest_close, "vwap": vwap, "ema9": latest_ema9, "ema21": latest_ema21, "macd": macd, "volume": latest_volume, "average_volume": typical_volume},
        "score": score,
        "max_score": 5,
        "verdict": "research candidate" if score >= 4 else "observe only",
        "evidence": evidence,
        "execution": "research_only_no_orders",
        "limitations": [
            "session timing is a falsifiable filter, not a universal edge",
            "OHLCV bars do not model order queue, fees, slippage or Polymarket fill mechanics",
            "the public strategy narrative is unverified and must be tested out of sample",
        ],
    }, ensure_ascii=False, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
