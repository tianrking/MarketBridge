#!/usr/bin/env python3
"""Replay BTC response after rolling public liquidation bursts in JSONL."""

import argparse
import json
import statistics
from pathlib import Path


def load_records(path):
    records, invalid_lines = [], 0
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                invalid_lines += 1
                continue
            if isinstance(record, dict) and isinstance(record.get("observation"), dict):
                records.append(record)
            else:
                invalid_lines += 1
    return records, invalid_lines


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def liquidation_rows(records):
    rows, seen = [], set()
    for record in records:
        for row in (record.get("observation") or {}).get("liquidations", []):
            ts_ms, notional = row.get("ts_ms"), number(row.get("notional"))
            if not isinstance(ts_ms, int) or notional is None or notional <= 0:
                continue
            key = (ts_ms, notional, number(row.get("price")),
                   str(row.get("side", "")).lower())
            if key in seen:
                continue
            seen.add(key)
            rows.append({"ts_ms": ts_ms, "notional": notional,
                         "price": number(row.get("price")), "side": row.get("side")})
    return sorted(rows, key=lambda row: row["ts_ms"])


def price_of(record):
    value = number((((record.get("observation") or {}).get("price") or {}).get("price")))
    return value if value is not None and value > 0 else None


def burst_observations(records, events, window_ms, horizon_records, threshold, cooldown_records):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    observations, last_trigger = [], None
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        current, future = price_of(record), price_of(ordered[future_index])
        ts_ms = record.get("recorded_at_ms")
        if current is None or future is None or not isinstance(ts_ms, int):
            continue
        selected = [row for row in events if ts_ms - window_ms < row["ts_ms"] <= ts_ms]
        total = sum(row["notional"] for row in selected)
        is_burst = total >= threshold
        if is_burst:
            if last_trigger is not None and index - last_trigger <= cooldown_records:
                continue
            last_trigger = index
        state = "liquidation_burst" if is_burst else "ordinary_liquidation_window"
        forward = (future / current - 1.0) * 100.0
        observations.append({
            "recorded_at_ms": ts_ms, "state": state,
            "window_notional": total, "event_count": len(selected),
            "sell_notional": sum(row["notional"] for row in selected
                                  if str(row.get("side", "")).lower() == "sell") or None,
            "buy_notional": sum(row["notional"] for row in selected
                                 if str(row.get("side", "")).lower() == "buy") or None,
            "forward_return_pct": forward, "forward_abs_return_pct": abs(forward),
        })
    return observations


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_window_notional": statistics.median(row["window_notional"] for row in rows)
        if rows else None,
    }


def summarize_records(records, window_ms, horizon_records, threshold, cooldown_records,
                      min_observations):
    events = liquidation_rows(records)
    observations = burst_observations(records, events, window_ms, horizon_records,
                                      threshold, cooldown_records)
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in ("liquidation_burst", "ordinary_liquidation_window")}
    burst = by_state["liquidation_burst"]["mean_absolute_forward_return_pct"]
    ordinary = by_state["ordinary_liquidation_window"]["mean_absolute_forward_return_pct"]
    edge_bps = (burst - ordinary) * 100.0 if burst is not None and ordinary is not None else None
    burst_count = by_state["liquidation_burst"]["observations"]
    return {
        "by_state": by_state, "unique_liquidation_events": len(events),
        "aligned_forward_windows": len(observations), "absolute_move_edge_bps": edge_bps,
        "verdict": ("liquidation_burst_response_reported" if burst_count >= min_observations
                    else "observe_only_insufficient_liquidation_bursts"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--window-hours", type=float, default=24.0)
    parser.add_argument("--horizon-records", type=int, default=12)
    parser.add_argument("--threshold-notional", type=float, default=1_000_000_000.0)
    parser.add_argument("--cooldown-records", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if (args.window_hours <= 0 or args.horizon_records <= 0 or args.threshold_notional < 0
            or args.cooldown_records < 0 or args.min_observations <= 0):
        parser.error("invalid window, threshold, horizon, cooldown or observation arguments")
    records, invalid_lines = load_records(args.input)
    summary = summarize_records(records, int(args.window_hours * 3_600_000),
                                args.horizon_records, args.threshold_notional,
                                args.cooldown_records, args.min_observations)
    print(json.dumps({
        "strategy": "crypto_liquidation_burst_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"window_hours": args.window_hours, "horizon_records": args.horizon_records,
                    "threshold_notional": args.threshold_notional,
                    "cooldown_records": args.cooldown_records,
                    "min_observations": args.min_observations},
        "summary": summary,
        "limitations": [
            "history is provider-bounded and repeated rows are deduplicated only by observable fields",
            "side labels remain metadata and do not prove long/short liquidation semantics",
            "fixed record horizons are not exact elapsed time; missing quotes are excluded",
            "no fees, fills, slippage, latency, position sizing or directional trade model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
