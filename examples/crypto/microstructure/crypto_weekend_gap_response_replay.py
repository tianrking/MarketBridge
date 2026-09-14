#!/usr/bin/env python3
"""Replay a historical weekend-reference dislocation response hypothesis.

The case uses continuous crypto candles to proxy the historical CME schedule:
Friday 16:00 America/Chicago close versus Sunday 17:00 America/Chicago open.
It asks whether large upward/downward weekend dislocations are more likely to
touch the Friday reference price during the next fixed candle window than
small-dislocation controls.  It does not claim a live CME gap or an executable
gap-fill trade, especially after the move toward 24/7 crypto derivatives.
"""

import argparse
import json
import statistics
import time
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo
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


def interval_millis(value):
    units = {"m": 60_000, "h": 3_600_000, "d": 86_400_000}
    if not value or value[-1:] not in units or not value[:-1].isdigit():
        return None
    return int(value[:-1]) * units[value[-1]]


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        values = {key: number(row.get(key)) for key in ("open", "high", "low", "close", "volume")}
        if (isinstance(timestamp, int) and all(value is not None for value in values.values())
                and values["open"] > 0 and values["close"] > 0
                and values["high"] >= values["low"] > 0
                and values["high"] >= values["open"] >= values["low"]
                and values["high"] >= values["close"] >= values["low"]
                and values["volume"] >= 0):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def local_datetime(ts_ms, timezone_name):
    return datetime.fromtimestamp(ts_ms / 1000.0, tz=timezone.utc).astimezone(
        ZoneInfo(timezone_name)
    )


def weekend_pairs(rows, timezone_name, interval_ms):
    """Pair Friday 15:00 candle close with Sunday 17:00 candle open.

    The Friday candle opens at 15:00 and closes at the 16:00 reference time;
    the Sunday candle opens at the 17:00 reference time.  Local calendar
    arithmetic keeps daylight-saving transitions explicit.
    """
    if interval_ms is None or interval_ms <= 0:
        return []
    by_local = {
        local_datetime(row["ts_ms"], timezone_name).replace(tzinfo=None): row
        for row in rows
    }
    sunday_rows = []
    for local_time, row in by_local.items():
        if local_time.weekday() == 6 and local_time.hour == 17 and local_time.minute == 0:
            friday_time = (local_time - timedelta(days=2)).replace(hour=15, minute=0)
            friday = by_local.get(friday_time)
            if friday is not None:
                sunday_rows.append({
                    "friday": friday,
                    "sunday": row,
                    "friday_local": friday_time.isoformat(),
                    "sunday_local": local_time.isoformat(),
                })
    return sorted(sunday_rows, key=lambda item: item["sunday"]["ts_ms"])


def weekend_observations(rows, timezone_name, interval_ms, horizon_bars,
                         min_gap_bps, fill_tolerance_bps):
    if horizon_bars <= 0 or min_gap_bps < 0 or fill_tolerance_bps < 0:
        return []
    by_ts = {row["ts_ms"]: row for row in rows}
    output = []
    for pair in weekend_pairs(rows, timezone_name, interval_ms):
        friday = pair["friday"]
        sunday = pair["sunday"]
        reference = friday["close"]
        sunday_open = sunday["open"]
        if reference <= 0 or sunday_open <= 0:
            continue
        gap_pct = (sunday_open / reference - 1.0) * 100.0
        gap_bps = gap_pct * 100.0
        state = ("up_dislocation" if gap_bps >= min_gap_bps else
                 "down_dislocation" if gap_bps <= -min_gap_bps else "small_dislocation")
        future_ts = sunday["ts_ms"] + horizon_bars * interval_ms
        future = by_ts.get(future_ts)
        if future is None or future["close"] <= 0:
            continue
        path = []
        for step in range(1, horizon_bars + 1):
            item = by_ts.get(sunday["ts_ms"] + step * interval_ms)
            if item is None:
                path = []
                break
            path.append(item)
        if len(path) != horizon_bars:
            continue
        tolerance = reference * fill_tolerance_bps / 10_000.0
        filled = any(row["low"] <= reference + tolerance and row["high"] >= reference - tolerance
                     for row in path)
        forward = (future["close"] / sunday_open - 1.0) * 100.0
        gap_direction = 1 if gap_bps > 0 else -1 if gap_bps < 0 else 0
        fill_direction = -gap_direction
        output.append({
            "friday_close_ts_ms": friday["ts_ms"],
            "sunday_open_ts_ms": sunday["ts_ms"],
            "friday_local": pair["friday_local"],
            "sunday_local": pair["sunday_local"],
            "friday_reference_price": reference,
            "sunday_reference_price": sunday_open,
            "gap_return_pct": gap_pct,
            "gap_bps": gap_bps,
            "state": state,
            "filled_reference": filled,
            "forward_ts_ms": future_ts,
            "forward_return_pct": forward,
            "fill_aligned_return_bps": fill_direction * forward * 100.0 if fill_direction else None,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(row["low"] for row in path)
                                             / sunday_open - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(row["high"] for row in path)
                                             / sunday_open - 1.0) * 100.0,
        })
    return output


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    aligned = [row["fill_aligned_return_bps"] for row in rows
               if row["fill_aligned_return_bps"] is not None]
    return {
        "observations": len(rows),
        "filled_count": sum(row["filled_reference"] for row in rows),
        "fill_rate": (sum(row["filled_reference"] for row in rows) / len(rows)
                      if rows else None),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "median_forward_return_pct": statistics.median(signed) if signed else None,
        "mean_fill_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "mean_absolute_return_pct": statistics.mean([abs(value) for value in signed]) if signed else None,
    }


def summarize(observations, min_observations):
    states = ("up_dislocation", "down_dislocation", "small_dislocation")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    dislocations = [row for row in observations if row["state"] != "small_dislocation"]
    controls = [row for row in observations if row["state"] == "small_dislocation"]
    sufficient = len(dislocations) >= min_observations and len(controls) >= min_observations
    return {
        "observations": len(observations),
        "dislocation_observations": len(dislocations),
        "small_dislocation_control_observations": len(controls),
        "by_state": by_state,
        "dislocations": bucket_stats(dislocations),
        "small_dislocation_controls": bucket_stats(controls),
        "verdict": ("historical_weekend_dislocation_reported" if sufficient
                     else "observe_only_insufficient_dislocation_or_control"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--timezone", default="America/Chicago")
    parser.add_argument("--days", type=float, default=730.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--pages", type=int, default=12)
    parser.add_argument("--min-gap-bps", type=float, default=50.0)
    parser.add_argument("--fill-tolerance-bps", type=float, default=5.0)
    parser.add_argument("--horizon-bars", type=int, default=24)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    interval_ms = interval_millis(args.interval)
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or not 1 <= args.pages <= 48
            or interval_ms is None
            or args.min_gap_bps < 0 or args.fill_tolerance_bps < 0
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid interval, gap, horizon or observation arguments")
    try:
        ZoneInfo(args.timezone)
    except Exception as error:
        parser.error(f"invalid timezone: {error}")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval,
        "start_ms": start_ms, "end_ms": end_ms, "limit": args.limit, "pages": args.pages,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = weekend_observations(
        rows, args.timezone, interval_ms, args.horizon_bars,
        args.min_gap_bps, args.fill_tolerance_bps,
    )
    coverage = payload.get("coverage_detail")
    evidence = ["historical_weekend_reference_and_forward_windows_available"
                if observations else "no_complete_weekend_reference_windows"]
    if isinstance(coverage, dict) and coverage.get("status"):
        evidence.append(f"candle_coverage_{coverage['status']}")
    print(json.dumps({
        "strategy": "crypto_weekend_gap_response_replay",
        "hypothesis": "large historical Friday-close to Sunday-open reference dislocations may touch the Friday reference more often than small-dislocation controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "reference": {"timezone": args.timezone, "friday_close": "16:00 local (15:00 candle close)",
                      "sunday_open": "17:00 local", "schedule_proxy": "historical_cme_reference"},
        "filters": {"min_gap_bps": args.min_gap_bps,
                    "pages": args.pages,
                    "fill_tolerance_bps": args.fill_tolerance_bps,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": [{"source": "candles", "error": payload["error"]}]
        if payload.get("error") else [],
        "limitations": [
            "continuous Binance candles are a proxy for a historical CME reference schedule, not CME prints",
            "the schedule and 24/7 product availability can change; this case must not be read as a live CME-gap signal",
            "Friday/Sunday local timestamps and candle completeness are required; missing bars are skipped",
            "reference touch is an OHLC path label and does not prove a fill, order, stop or executable gap trade",
            "thresholds, horizon, venue, timezone and overlapping weekly samples are sensitivity choices",
            "fees, funding, borrow, latency, slippage, queue, custody and execution are not inferred",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
