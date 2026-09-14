#!/usr/bin/env python3
"""Replay session-VWAP deviation cross-back responses from candle history."""

import argparse
import json
import statistics
import time
from datetime import datetime, timedelta, timezone
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
        timestamp = row.get("open_time_ms")
        close, high, low, volume = (number(row.get(key)) for key in ("close", "high", "low", "volume"))
        if (isinstance(timestamp, int) and close is not None and close > 0
                and high is not None and low is not None and high >= low > 0
                and volume is not None and volume > 0):
            rows.append({"ts_ms": timestamp, "close": close, "high": high,
                         "low": low, "volume": volume})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def session_date(timestamp):
    # Avoid platform-specific C-runtime handling of Unix epoch timestamps on
    # Windows (the deterministic tests intentionally include timestamp=0).
    epoch = datetime(1970, 1, 1, tzinfo=timezone.utc)
    return (epoch + timedelta(milliseconds=timestamp)).date()


def update_session(state, row):
    typical = (row["high"] + row["low"] + row["close"]) / 3.0
    state["sum_volume"] += row["volume"]
    state["sum_price_volume"] += typical * row["volume"]
    state["sum_squared_price_volume"] += typical * typical * row["volume"]
    vwap = state["sum_price_volume"] / state["sum_volume"]
    variance = max(state["sum_squared_price_volume"] / state["sum_volume"] - vwap * vwap, 0.0)
    return {"vwap": vwap, "std": variance ** 0.5, "typical": typical}


def reversion_events(rows, deviation_bps, sigma, horizon_bars):
    events = []
    state = {"day": None, "sum_volume": 0.0, "sum_price_volume": 0.0,
             "sum_squared_price_volume": 0.0}
    previous = None
    for index, row in enumerate(rows):
        day = session_date(row["ts_ms"])
        if state["day"] != day:
            state = {"day": day, "sum_volume": 0.0, "sum_price_volume": 0.0,
                     "sum_squared_price_volume": 0.0}
            previous = None
        current = update_session(state, row)
        if previous is not None and previous["day"] == day:
            threshold = max(previous["vwap"] * deviation_bps / 10_000.0,
                            previous["std"] * sigma)
            long_signal = (previous["close"] <= previous["vwap"] - threshold
                           and row["close"] >= current["vwap"])
            short_signal = (previous["close"] >= previous["vwap"] + threshold
                            and row["close"] <= current["vwap"])
            if (long_signal or short_signal) and index + horizon_bars < len(rows):
                future = rows[index + horizon_bars]
                forward = (future["close"] / row["close"] - 1.0) * 100.0
                direction = 1 if long_signal else -1
                events.append({
                    "ts_ms": row["ts_ms"], "direction": "long" if direction > 0 else "short",
                    "direction_sign": direction, "previous_vwap": previous["vwap"],
                    "previous_std": previous["std"], "trigger_vwap": current["vwap"],
                    "trigger_close": row["close"], "forward_return_pct": forward,
                    "aligned_return_bps": direction * forward * 100.0,
                })
        previous = {"day": day, "close": row["close"], "vwap": current["vwap"], "std": current["std"]}
    return events


def summarize(events, paper_cost_bps, min_edge_bps, min_observations):
    gross = [row["aligned_return_bps"] for row in events]
    adjusted = [value - paper_cost_bps for value in gross]
    mean_adjusted = statistics.mean(adjusted) if adjusted else None
    candidate = (len(adjusted) >= min_observations and mean_adjusted is not None
                 and mean_adjusted >= min_edge_bps)
    return {
        "signals": len(events), "long_signals": sum(row["direction_sign"] > 0 for row in events),
        "short_signals": sum(row["direction_sign"] < 0 for row in events),
        "aligned_hit_rate": (sum(value > 0 for value in gross) / len(gross) if gross else None),
        "mean_aligned_return_bps": statistics.mean(gross) if gross else None,
        "median_aligned_return_bps": statistics.median(gross) if gross else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_return_bps": mean_adjusted,
        "min_edge_bps": min_edge_bps,
        "verdict": "vwap_deviation_response_reported" if candidate else "observe_only",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=30.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--deviation-bps", type=float, default=50.0)
    parser.add_argument("--sigma", type=float, default=2.0)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.deviation_bps < 0
            or args.sigma < 0 or args.horizon_bars <= 0 or args.paper_cost_bps < 0
            or args.min_edge_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid VWAP, horizon, cost or history arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "symbol": args.symbol, "market": args.market,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    events = reversion_events(rows, args.deviation_bps, args.sigma, args.horizon_bars)
    coverage = payload.get("coverage_detail")
    evidence = ["historical_candles_available" if rows else "missing_historical_candles"]
    if isinstance(coverage, dict) and coverage.get("status"):
        evidence.append(f"candle_coverage_{coverage['status']}")
    print(json.dumps({
        "strategy": "crypto_vwap_deviation_reversion_replay",
        "hypothesis": "a prior session-VWAP deviation followed by a cross-back may show directional mean-reversion response",
        "market": {"exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                   "interval": args.interval, "session_timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"deviation_bps": args.deviation_bps, "sigma": args.sigma,
                    "horizon_bars": args.horizon_bars, "paper_cost_bps": args.paper_cost_bps,
                    "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "signals": len(events)},
        "coverage": coverage, "observations": events,
        "summary": summarize(events, args.paper_cost_bps, args.min_edge_bps, args.min_observations),
        "evidence": evidence,
        "upstream_errors": [payload.get("error")] if payload.get("error") else [],
        "limitations": [
            "session resets at UTC midnight and uses OHLCV typical-price volume weighting",
            "cross-back is measured at candle close and may overlap other signals",
            "fixed close-to-close returns are not fills, position PnL or a stop model",
            "paper cost is a sensitivity hurdle, not fees, slippage, funding or execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
