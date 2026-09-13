#!/usr/bin/env python3
"""Replay BTC movement after observed liquidation price-band clusters."""

import argparse
import json
import statistics
from pathlib import Path

from crypto_liquidation_price_cluster_replay import cluster_profile


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
            ts_ms, price, notional = row.get("ts_ms"), number(row.get("price")), number(row.get("notional"))
            if not isinstance(ts_ms, int) or price is None or price <= 0 or notional is None or notional <= 0:
                continue
            key = (ts_ms, price, notional, str(row.get("side", "")).lower())
            if key in seen:
                continue
            seen.add(key)
            rows.append({"ts_ms": ts_ms, "price": price, "notional": notional,
                         "side": row.get("side")})
    return sorted(rows, key=lambda row: row["ts_ms"])


def price_of(record):
    value = number((((record.get("observation") or {}).get("price") or {}).get("price")))
    return value if value is not None and value > 0 else None


def cluster_at(events, ts_ms, window_ms, threshold, band_bps, min_share):
    selected = [row for row in events if ts_ms - window_ms < row["ts_ms"] <= ts_ms]
    profile = cluster_profile(selected, band_bps)
    if profile is None or profile["total_notional"] < threshold or profile["cluster_share"] < min_share:
        return None
    return profile


def cluster_observations(records, events, window_ms, horizon_records, threshold, band_bps,
                         min_share, cooldown_records):
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
        profile = cluster_at(events, ts_ms, window_ms, threshold, band_bps, min_share)
        if profile is not None:
            if last_trigger is not None and index - last_trigger <= cooldown_records:
                continue
            last_trigger = index
            state = "liquidation_price_cluster"
        else:
            state = "ordinary_liquidation_window"
        forward = (future / current - 1.0) * 100.0
        row = {"state": state, "forward_return_pct": forward,
               "forward_abs_return_pct": abs(forward)}
        if profile is not None:
            row.update({"cluster_share": profile["cluster_share"],
                        "cluster_center_price": profile["center_price"],
                        "distance_to_cluster_bps": (profile["center_price"] / current - 1.0) * 10_000.0,
                        "event_count": profile["cluster_events"]})
        observations.append(row)
    return observations


def stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
        "median_cluster_share": statistics.median(row["cluster_share"] for row in rows
                                                   if "cluster_share" in row)
        if any("cluster_share" in row for row in rows) else None,
        "median_distance_to_cluster_bps": statistics.median(
            row["distance_to_cluster_bps"] for row in rows if "distance_to_cluster_bps" in row
        ) if any("distance_to_cluster_bps" in row for row in rows) else None,
    }


def summarize_records(records, window_ms, horizon_records, threshold, band_bps, min_share,
                      cooldown_records, min_observations):
    events = liquidation_rows(records)
    observations = cluster_observations(records, events, window_ms, horizon_records,
                                        threshold, band_bps, min_share, cooldown_records)
    by_state = {state: stats([row for row in observations if row["state"] == state])
                for state in ("liquidation_price_cluster", "ordinary_liquidation_window")}
    cluster_move = by_state["liquidation_price_cluster"]["mean_absolute_forward_return_pct"]
    ordinary_move = by_state["ordinary_liquidation_window"]["mean_absolute_forward_return_pct"]
    return {
        "snapshots": len(records), "unique_liquidation_events": len(events),
        "aligned_forward_windows": len(observations), "by_state": by_state,
        "cluster_absolute_move_edge_bps": ((cluster_move - ordinary_move) * 100.0
                                            if cluster_move is not None and ordinary_move is not None else None),
        "verdict": ("liquidation_price_cluster_response_reported"
                    if by_state["liquidation_price_cluster"]["observations"] >= min_observations
                    else "observe_only_insufficient_price_clusters"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--window-hours", type=float, default=24.0)
    parser.add_argument("--horizon-records", type=int, default=12)
    parser.add_argument("--threshold-notional", type=float, default=1_000_000_000.0)
    parser.add_argument("--cluster-band-bps", type=float, default=25.0)
    parser.add_argument("--min-cluster-share", type=float, default=0.5)
    parser.add_argument("--cooldown-records", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if (args.window_hours <= 0 or args.horizon_records <= 0 or args.threshold_notional < 0
            or args.cluster_band_bps <= 0 or not 0 <= args.min_cluster_share <= 1
            or args.cooldown_records < 0 or args.min_observations <= 0):
        parser.error("invalid window, threshold, cluster, cooldown, horizon or observation arguments")
    records, invalid_lines = load_records(args.input)
    summary = summarize_records(records, int(args.window_hours * 3_600_000), args.horizon_records,
                                args.threshold_notional, args.cluster_band_bps,
                                args.min_cluster_share, args.cooldown_records,
                                args.min_observations)
    print(json.dumps({"strategy": "crypto_liquidation_price_cluster_response_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"window_hours": args.window_hours,
                                  "horizon_records": args.horizon_records,
                                  "threshold_notional": args.threshold_notional,
                                  "cluster_band_bps": args.cluster_band_bps,
                                  "min_cluster_share": args.min_cluster_share,
                                  "cooldown_records": args.cooldown_records,
                                  "min_observations": args.min_observations},
                      "summary": summary,
                      "limitations": [
                          "only observed liquidation prints are clustered; latent heatmap levels are not reconstructed",
                          "repeated rows are deduplicated by observable timestamp, price, notional and side",
                          "fixed record horizons are not exact elapsed time and missing quotes are excluded",
                          "no liquidation forecast, fees, fills, slippage, position sizing or execution model",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
