#!/usr/bin/env python3
"""Replay a weekday/hour BTC return hypothesis with a matched clock control.

The public lead claims a recurring Tuesday 05:00 UTC sell-off followed by a
small bounce and later weakness.  This replay keeps the claim falsifiable:
target candles are compared with all other weekdays at the same UTC hour,
using only historical OHLCV and fixed close-to-close windows.  It does not
infer the actor, cause, fills or a trading instruction.
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
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        open_price, close = number(row.get("open")), number(row.get("close"))
        high, low = number(row.get("high")), number(row.get("low"))
        if (isinstance(ts_ms, int) and open_price is not None and close is not None
                and high is not None and low is not None
                and open_price > 0 and close > 0 and high >= low > 0):
            rows.append({"ts_ms": ts_ms, "open": open_price, "close": close,
                         "high": high, "low": low})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def event_rows(rows, target_weekday, target_hour, horizon_hours, bounce_hours):
    by_ts = {row["ts_ms"]: row for row in rows}
    hour_ms = 3_600_000
    output = []
    for row in rows:
        utc = time.gmtime(row["ts_ms"] / 1000.0)
        if utc.tm_hour != target_hour:
            continue
        future = by_ts.get(row["ts_ms"] + horizon_hours * hour_ms)
        bounce = by_ts.get(row["ts_ms"] + bounce_hours * hour_ms)
        if future is None or bounce is None:
            continue
        target = utc.tm_wday == target_weekday
        output.append({
            "ts_ms": row["ts_ms"],
            "weekday": utc.tm_wday,
            "hour_utc": utc.tm_hour,
            "bucket": "target" if target else "same_hour_control",
            "event_return_pct": (row["close"] / row["open"] - 1.0) * 100.0,
            "bounce_return_pct": (bounce["close"] / row["close"] - 1.0) * 100.0,
            "forward_return_pct": (future["close"] / row["close"] - 1.0) * 100.0,
            "red_event": row["close"] < row["open"],
        })
    return output


def bucket_stats(events, paper_cost_bps):
    event_returns = [row["event_return_pct"] for row in events]
    bounce_returns = [row["bounce_return_pct"] for row in events]
    forward_returns = [row["forward_return_pct"] for row in events]
    adjusted = [value - paper_cost_bps / 100.0 for value in forward_returns]
    return {
        "observations": len(events),
        "red_event_fraction": (sum(row["red_event"] for row in events) / len(events)
                               if events else None),
        "mean_event_return_pct": statistics.mean(event_returns) if event_returns else None,
        "median_event_return_pct": statistics.median(event_returns) if event_returns else None,
        "mean_bounce_return_pct": statistics.mean(bounce_returns) if bounce_returns else None,
        "mean_forward_return_pct": statistics.mean(forward_returns) if forward_returns else None,
        "median_forward_return_pct": statistics.median(forward_returns) if forward_returns else None,
        "mean_absolute_forward_return_pct": (statistics.mean(abs(value) for value in forward_returns)
                                             if forward_returns else None),
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_forward_return_pct": statistics.mean(adjusted) if adjusted else None,
    }


def summarize(events, min_observations, paper_cost_bps):
    target = [row for row in events if row["bucket"] == "target"]
    control = [row for row in events if row["bucket"] == "same_hour_control"]
    target_stats, control_stats = bucket_stats(target, paper_cost_bps), bucket_stats(control, paper_cost_bps)
    target_forward = target_stats["mean_forward_return_pct"]
    control_forward = control_stats["mean_forward_return_pct"]
    enough = len(target) >= min_observations and len(control) >= min_observations
    return {
        "target": target_stats,
        "same_hour_control": control_stats,
        "forward_mean_difference_target_minus_control_pct": (
            target_forward - control_forward
            if target_forward is not None and control_forward is not None else None
        ),
        "verdict": "weekday_hour_effect_reported" if enough
        else "observe_only_insufficient_matched_clock_observations",
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
    parser.add_argument("--target-weekday", type=int, default=1,
                        help="UTC weekday: Monday=0, Tuesday=1, ..., Sunday=6")
    parser.add_argument("--target-hour", type=int, default=5)
    parser.add_argument("--horizon-hours", type=int, default=8)
    parser.add_argument("--bounce-hours", type=int, default=1)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or not 0 <= args.target_weekday <= 6
            or not 0 <= args.target_hour <= 23 or args.horizon_hours <= 0
            or args.bounce_hours <= 0 or args.paper_cost_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, weekday/hour, horizon, cost or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    events = event_rows(rows, args.target_weekday, args.target_hour,
                        args.horizon_hours, args.bounce_hours)
    summary = summarize(events, args.min_observations, args.paper_cost_bps)
    target_events = sum(row["bucket"] == "target" for row in events)
    evidence = ["matched_clock_candles_available" if events else "missing_matched_clock_candles"]
    coverage = payload.get("coverage_detail")
    if isinstance(coverage, dict) and coverage.get("status"):
        evidence.append(f"candle_coverage_{coverage['status']}")
    print(json.dumps({
        "strategy": "crypto_weekday_hour_effect_replay",
        "hypothesis": "the selected weekday/hour has a different event, bounce and forward response than other weekdays at the same UTC hour",
        "market": {"exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                   "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"target_weekday": args.target_weekday, "target_hour": args.target_hour,
                    "horizon_hours": args.horizon_hours, "bounce_hours": args.bounce_hours,
                    "paper_cost_bps": args.paper_cost_bps, "min_observations": args.min_observations},
        "source_counts": {"candles": len(rows), "matched_events": len(events),
                           "target_events": target_events},
        "observations": events,
        "summary": summary,
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "weekday/hour association is descriptive and does not identify an actor or cause",
            "same-hour controls share the same UTC clock but not the same weekday or market regime",
            "missing bars, timezone choice, sample length, fees, funding, slippage and fills remain explicit gaps",
            "fixed OHLCV returns are not an entry, stop, allocation or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
