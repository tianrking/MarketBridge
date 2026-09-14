#!/usr/bin/env python3
"""Replay BTC response after rolling public transfer bursts in a JSONL archive."""

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


def transfer_rows(records):
    rows, seen = [], set()
    for record in records:
        for row in (record.get("observation") or {}).get("transfers", []):
            ts_ms = row.get("ts_ms")
            amount = number(row.get("amount_usd"))
            if not isinstance(ts_ms, int) or amount is None or amount <= 0:
                continue
            key = (ts_ms, amount, str(row.get("asset", "")).upper(),
                   str(row.get("chain", "")).lower(), str(row.get("source", "")).lower(),
                   str(row.get("direction", "")).lower())
            if key in seen:
                continue
            seen.add(key)
            rows.append({"ts_ms": ts_ms, "amount_usd": amount,
                         "asset": row.get("asset"), "chain": row.get("chain"),
                         "direction": row.get("direction"), "source": row.get("source")})
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
        if current is None or future is None:
            continue
        ts_ms = record.get("recorded_at_ms")
        if not isinstance(ts_ms, int):
            continue
        selected = [row for row in events if ts_ms - window_ms < row["ts_ms"] <= ts_ms]
        total = sum(row["amount_usd"] for row in selected)
        if total >= threshold:
            if last_trigger is not None and index - last_trigger <= cooldown_records:
                continue
            state = "transfer_burst"
            last_trigger = index
        else:
            state = "ordinary_transfer_window"
        observations.append({
            "recorded_at_ms": ts_ms, "state": state, "window_amount_usd": total,
            "event_count": len(selected), "forward_return_pct": (future / current - 1.0) * 100.0,
            "forward_abs_return_pct": abs((future / current - 1.0) * 100.0),
            "assets": sorted({str(row["asset"]).upper() for row in selected if row.get("asset")}),
            "chains": sorted({str(row["chain"]).lower() for row in selected if row.get("chain")}),
        })
    return observations


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_window_amount_usd": statistics.median(row["window_amount_usd"] for row in rows)
        if rows else None,
    }


def summarize_records(records, window_ms, horizon_records, threshold, cooldown_records, min_observations):
    events = transfer_rows(records)
    observations = burst_observations(records, events, window_ms, horizon_records,
                                      threshold, cooldown_records)
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in ("transfer_burst", "ordinary_transfer_window")}
    burst = by_state["transfer_burst"]["mean_absolute_forward_return_pct"]
    ordinary = by_state["ordinary_transfer_window"]["mean_absolute_forward_return_pct"]
    edge_bps = (burst - ordinary) * 100.0 if burst is not None and ordinary is not None else None
    return {
        "by_state": by_state, "unique_transfer_events": len(events),
        "aligned_forward_windows": len(observations), "absolute_move_edge_bps": edge_bps,
        "verdict": ("onchain_transfer_response_reported"
                    if len([row for row in observations if row["state"] == "transfer_burst"]) >= min_observations
                    else "observe_only_insufficient_transfer_bursts"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--window-hours", type=float, default=24.0)
    parser.add_argument("--horizon-records", type=int, default=12)
    parser.add_argument("--threshold-usd", type=float, default=1_000_000.0)
    parser.add_argument("--cooldown-records", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if (args.window_hours <= 0 or args.horizon_records <= 0 or args.threshold_usd < 0
            or args.cooldown_records < 0 or args.min_observations <= 0):
        parser.error("invalid window, threshold, horizon, cooldown or observation arguments")
    records, invalid_lines = load_records(args.input)
    summary = summarize_records(records, int(args.window_hours * 3_600_000),
                                args.horizon_records, args.threshold_usd,
                                args.cooldown_records, args.min_observations)
    print(json.dumps({
        "strategy": "crypto_onchain_transfer_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"window_hours": args.window_hours, "horizon_records": args.horizon_records,
                    "threshold_usd": args.threshold_usd, "cooldown_records": args.cooldown_records,
                    "min_observations": args.min_observations},
        "summary": summary,
        "limitations": [
            "transfer snapshots are provider- and configuration-dependent, not a full-chain ledger",
            "events can be repeated across snapshots and are deduplicated only by observable fields",
            "direction, address labels and asset type do not prove exchange net flow or causality",
            "no fees, fills, gas, latency, wallet action, sizing or directional trade model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
