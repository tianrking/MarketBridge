#!/usr/bin/env python3
"""Replay a session VWAP/EMA/MACD/volume hypothesis without execution.

The falsifiable question is whether a directional confluence during a selected
local-time session window aligns with the next fixed number of candle bars.
This is a bounded close-to-close event study, not an entry engine or fill model.
"""

import argparse
import json
import statistics
import time
from datetime import datetime, time as clock_time, timezone
from zoneinfo import ZoneInfo
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close, high, low, volume = (number(row.get(key)) for key in ("close", "high", "low", "volume"))
        if (isinstance(ts_ms, int) and close is not None and high is not None and low is not None
                and close > 0 and high > 0 and low > 0):
            rows.append({"ts_ms": ts_ms, "close": close, "high": high, "low": low,
                         "volume": volume if volume is not None and volume >= 0 else None})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def ema_series(values, period):
    result = [None] * len(values)
    if len(values) < period:
        return result
    current = sum(values[:period]) / period
    result[period - 1] = current
    multiplier = 2.0 / (period + 1.0)
    for index in range(period, len(values)):
        current = (values[index] - current) * multiplier + current
        result[index] = current
    return result


def parse_clock(value):
    try:
        hour, minute = (int(part) for part in value.split(":", 1))
        return clock_time(hour, minute)
    except (TypeError, ValueError):
        raise ValueError("session times must use HH:MM")


def session_key(dt, start, end):
    if start <= end:
        return dt.date() if dt.time() >= start else None
    if dt.time() >= start:
        return dt.date()
    if dt.time() < end:
        return dt.date().toordinal() - 1
    return None


def in_session(dt, start, end):
    if start <= end:
        return start <= dt.time() < end
    return dt.time() >= start or dt.time() < end


def confluence_observations(rows, timezone_name, session_start, session_end,
                            volume_multiplier, horizon_bars, min_score,
                            volume_window=20):
    if not rows:
        return []
    location = ZoneInfo(timezone_name)
    closes = [row["close"] for row in rows]
    ema9, ema21 = ema_series(closes, 9), ema_series(closes, 21)
    macd = [a - b if a is not None and b is not None else None for a, b in zip(ema9, ema21)]
    by_ts = {row["ts_ms"]: row for row in rows}
    output, vwap_state = [], {}
    for index, row in enumerate(rows):
        dt = datetime.fromtimestamp(row["ts_ms"] / 1000.0, tz=timezone.utc).astimezone(location)
        if not in_session(dt, session_start, session_end):
            continue
        key = session_key(dt, session_start, session_end)
        if key is None:
            continue
        state = vwap_state.setdefault(key, [0.0, 0.0])
        if row["volume"] is not None:
            state[0] += ((row["high"] + row["low"] + row["close"]) / 3.0) * row["volume"]
            state[1] += row["volume"]
        vwap = state[0] / state[1] if state[1] > 0 else None
        if (index < 1 or ema9[index] is None or ema21[index] is None or macd[index] is None
                or macd[index - 1] is None or vwap is None):
            continue
        recent = [item["volume"] for item in rows[max(0, index - volume_window):index]
                  if item["volume"] is not None]
        average_volume = statistics.mean(recent) if recent else None
        volume_ok = (row["volume"] is not None and average_volume is not None
                     and row["volume"] >= average_volume * volume_multiplier)
        bullish = [row["close"] > vwap, ema9[index] > ema21[index], macd[index] > macd[index - 1], volume_ok]
        bearish = [row["close"] < vwap, ema9[index] < ema21[index], macd[index] < macd[index - 1], volume_ok]
        bull_score, bear_score = sum(bullish), sum(bearish)
        direction = 1 if bull_score > bear_score and bull_score + 1 >= min_score else (
            -1 if bear_score > bull_score and bear_score + 1 >= min_score else 0)
        if direction == 0:
            continue
        forward_ts = row["ts_ms"] + horizon_bars * 60_000
        future = by_ts.get(forward_ts)
        if future is None:
            continue
        forward_return_pct = (future["close"] / row["close"] - 1.0) * 100.0
        aligned = direction * forward_return_pct
        output.append({
            "ts_ms": row["ts_ms"], "forward_ts_ms": forward_ts,
            "direction": "long" if direction > 0 else "short",
            "score": max(bull_score, bear_score) + 1,
            "vwap": vwap, "ema9": ema9[index], "ema21": ema21[index],
            "macd": macd[index], "volume": row["volume"], "average_volume": average_volume,
            "forward_return_pct": forward_return_pct,
            "aligned_return_bps": aligned * 100.0,
            "aligned": aligned > 0,
        })
    return output


def summarize(observations, min_observations, paper_cost_bps, min_edge_bps):
    gross = [row["aligned_return_bps"] for row in observations]
    adjusted = [value - paper_cost_bps for value in gross]
    mean_adjusted = statistics.mean(adjusted) if adjusted else None
    candidate = (len(adjusted) >= min_observations and mean_adjusted is not None
                 and mean_adjusted >= min_edge_bps)
    return {
        "signals": len(observations), "aligned_signals": sum(row["aligned"] for row in observations),
        "hit_rate": (sum(row["aligned"] for row in observations) / len(observations)
                     if observations else None),
        "mean_aligned_return_bps": statistics.mean(gross) if gross else None,
        "median_aligned_return_bps": statistics.median(gross) if gross else None,
        "paper_cost_bps": paper_cost_bps, "mean_cost_adjusted_return_bps": mean_adjusted,
        "min_edge_bps": min_edge_bps,
        "verdict": "session_confluence_candidate" if candidate else "observe_only",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1m")
    parser.add_argument("--days", type=float, default=3.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--timezone", default="America/New_York")
    parser.add_argument("--session-start", default="09:00")
    parser.add_argument("--session-end", default="09:15")
    parser.add_argument("--volume-multiplier", type=float, default=1.0)
    parser.add_argument("--horizon-bars", type=int, default=15)
    parser.add_argument("--min-score", type=int, default=4)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit < 21 or args.volume_multiplier < 0 or args.horizon_bars <= 0
            or not 1 <= args.min_score <= 5 or args.paper_cost_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid session, horizon, score, cost or observation arguments")
    try:
        start, end = parse_clock(args.session_start), parse_clock(args.session_end)
    except ValueError as error:
        parser.error(str(error))
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.limit, 1500),
    }, args.timeout)
    rows = candle_rows(payload)
    observations = confluence_observations(rows, args.timezone, start, end,
                                           args.volume_multiplier, args.horizon_bars, args.min_score)
    summary = summarize(observations, args.min_observations, args.paper_cost_bps, args.min_edge_bps)
    evidence = ["session_features_and_forward_returns_available" if observations else "no_qualifying_session_features"]
    detail = payload.get("coverage_detail")
    if isinstance(detail, dict) and detail.get("status"):
        evidence.append(f"candle_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_session_momentum_replay",
        "market": {"exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                   "interval": args.interval},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "session": {"timezone": args.timezone, "start": args.session_start, "end": args.session_end},
        "filters": {"volume_multiplier": args.volume_multiplier, "horizon_bars": args.horizon_bars,
                    "min_score": args.min_score, "paper_cost_bps": args.paper_cost_bps,
                    "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations},
        "source_counts": {"candles": len(rows)}, "observations": observations,
        "summary": summary, "coverage": payload.get("coverage_detail"), "evidence": evidence,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "VWAP is session-window OHLCV typical-price VWAP, not executable order-flow VWAP",
            "EMA/MACD/volume confluence is a descriptive filter with no causal guarantee",
            "missing bars, timezone choice, fees, funding, slippage and fills remain explicit gaps",
            "no order, wallet, allocation or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
