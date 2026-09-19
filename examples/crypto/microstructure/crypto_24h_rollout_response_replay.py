#!/usr/bin/env python3
"""Replay the Binance perpetual 24-hour display roll-out hypothesis.

The public replication package claims that when the largest positive/negative
hourly candle leaves a rolling 24-hour display, the next hour has a different
direction-adjusted response.  This example keeps the claim narrow: it uses
completed hourly OHLCV candles, labels the 24-hour-old extreme, and measures
the next completed candle.  It is a research report, never an order or
execution instruction.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


HOUR_MS = 3_600_000


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
    return result if result > 0 else None


def candle_rows(payload):
    rows = []
    for item in payload.get("candles", []):
        ts_ms = item.get("open_time_ms")
        open_price, close = number(item.get("open")), number(item.get("close"))
        if isinstance(ts_ms, int) and open_price is not None and close is not None:
            rows.append({"ts_ms": ts_ms, "open": open_price, "close": close})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def rollout_events(rows, lookback_hours=24):
    """Return one event per contiguous hourly roll-out observation.

    The aged candle is compared with the preceding ``lookback_hours`` candle
    returns, including itself.  A positive maximum creates a short-side
    direction; a negative minimum creates a long-side direction.  Ties are
    retained as a deterministic observation rather than silently discarded.
    """
    by_ts = {row["ts_ms"]: row for row in rows}
    returns = {
        row["ts_ms"]: row["close"] / rows[index - 1]["close"] - 1.0
        for index, row in enumerate(rows)
        if index > 0 and row["ts_ms"] - rows[index - 1]["ts_ms"] == HOUR_MS
    }
    events = []
    for row in rows:
        entry_ts = row["ts_ms"]
        aged_ts = entry_ts - lookback_hours * HOUR_MS
        next_row = by_ts.get(entry_ts + HOUR_MS)
        aged_return = returns.get(aged_ts)
        if next_row is None or aged_return is None:
            continue
        window = [returns.get(aged_ts - offset * HOUR_MS) for offset in range(lookback_hours)]
        if any(value is None for value in window):
            continue
        if aged_return == max(window) and aged_return > 0:
            side = "short"
        elif aged_return == min(window) and aged_return < 0:
            side = "long"
        else:
            continue
        forward_return = next_row["close"] / row["close"] - 1.0
        direction_return = -forward_return if side == "short" else forward_return
        events.append({
            "ts_ms": entry_ts,
            "aged_candle_ts_ms": aged_ts,
            "side": side,
            "aged_candle_return_pct": aged_return * 100.0,
            "forward_return_pct": forward_return * 100.0,
            "direction_return_pct": direction_return * 100.0,
            "trigger_abs_return_pct": abs(aged_return) * 100.0,
        })
    return events


def side_stats(events, paper_cost_bps):
    returns = [row["direction_return_pct"] for row in events]
    adjusted = [value - paper_cost_bps / 100.0 for value in returns]
    return {
        "observations": len(events),
        "mean_direction_return_pct": statistics.mean(returns) if returns else None,
        "median_direction_return_pct": statistics.median(returns) if returns else None,
        "win_rate": (sum(value > 0 for value in returns) / len(returns) if returns else None),
        "mean_paper_cost_adjusted_return_pct": statistics.mean(adjusted) if adjusted else None,
        "paper_cost_bps": paper_cost_bps,
    }


def summarize(events, min_observations, paper_cost_bps):
    short = [row for row in events if row["side"] == "short"]
    long = [row for row in events if row["side"] == "long"]
    enough = len(events) >= min_observations
    return {
        "all": side_stats(events, paper_cost_bps),
        "short": side_stats(short, paper_cost_bps),
        "long": side_stats(long, paper_cost_bps),
        "verdict": "rollout_response_reported" if enough
        else "observe_only_insufficient_rollout_observations",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=60.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 49 <= args.limit <= 1500 or args.paper_cost_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid days, candle limit, cost, observation or timeout arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    events = rollout_events(rows)
    coverage = payload.get("coverage_detail")
    evidence = ["hourly_rollout_candidates_available" if events
                else "missing_contiguous_hourly_rollout_candidates"]
    if isinstance(coverage, dict) and coverage.get("status"):
        evidence.append(f"candle_coverage_{coverage['status']}")
    print(json.dumps({
        "strategy": "crypto_24h_rollout_response_replay",
        "hypothesis": "the candle leaving the rolling 24-hour display is followed by a direction-adjusted one-hour response",
        "market": {"exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                   "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"lookback_hours": 24, "paper_cost_bps": args.paper_cost_bps,
                    "min_observations": args.min_observations},
        "source_counts": {"candles": len(rows), "events": len(events)},
        "observations": events,
        "summary": summarize(events, args.min_observations, args.paper_cost_bps),
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": payload.get("errors", []),
        "provenance": {
            "research": "https://github.com/OctopusTakopi/24h-rollout-effect",
            "source_data": "MarketBridge history candles; original study used Binance public archives",
        },
        "limitations": [
            "this replay is a small single-symbol response study, not the original multi-symbol archive study",
            "the displayed 24-hour statistic and participant behavior are not observed directly",
            "missing bars, funding, fees, spread, slippage, capacity and tail risk remain explicit gaps",
            "direction-adjusted returns are descriptive and are not an entry, sizing, stop or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
