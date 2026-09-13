#!/usr/bin/env python3
"""Replay whether extreme recorded basis values subsequently contract.

The falsifiable hypothesis is narrow: after a basis observation is unusually
far from its trailing same-venue mean, the absolute basis may shrink over the
next few snapshots. This is descriptive basis behavior, not a carry PnL or
delta-neutral execution simulation.
"""

import argparse
import json
import math
import statistics
from pathlib import Path


def numeric(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def load_records(path):
    records = []
    invalid_lines = 0
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


def basis_series(records, symbol):
    series = {}
    for record in records:
        recorded_at = record.get("recorded_at_ms")
        if not isinstance(recorded_at, int):
            continue
        observation = record.get("observation") or {}
        for row in observation.get("basis", []):
            if str(row.get("symbol", "")).upper() != symbol.upper():
                continue
            exchange = str(row.get("exchange", "")).lower()
            basis_bps = numeric(row.get("basis_bps"))
            if exchange and basis_bps is not None and math.isfinite(basis_bps):
                series.setdefault(exchange, []).append((recorded_at, basis_bps))
    return {
        exchange: sorted(points)
        for exchange, points in series.items()
    }


def score_series(points, lookback, horizon, min_z):
    points = sorted(points)
    signals = []
    for index in range(lookback, len(points) - horizon):
        history = [value for _, value in points[index - lookback:index]]
        deviation = statistics.pstdev(history)
        if deviation <= 0:
            continue
        timestamp, current = points[index]
        mean = statistics.mean(history)
        z_score = (current - mean) / deviation
        if abs(z_score) < min_z:
            continue
        future = points[index + horizon][1]
        signals.append({
            "ts_ms": timestamp,
            "basis_bps": current,
            "trailing_mean_bps": mean,
            "z_score": z_score,
            "future_basis_bps": future,
            "absolute_change_bps": abs(future) - abs(current),
            "contracted": abs(future) < abs(current),
        })
    return signals


def summarize_series(series, lookback, horizon, min_z, min_signals, min_reversion_rate):
    by_exchange = {}
    for exchange, points in sorted(series.items()):
        signals = score_series(points, lookback, horizon, min_z)
        contractions = sum(item["contracted"] for item in signals)
        changes = [item["absolute_change_bps"] for item in signals]
        by_exchange[exchange] = {
            "input_points": len(points),
            "signals": len(signals),
            "contractions": contractions,
            "contraction_rate": contractions / len(signals) if signals else None,
            "median_absolute_change_bps": statistics.median(changes) if changes else None,
            "max_abs_z_score": max((abs(item["z_score"]) for item in signals), default=None),
            "events": signals,
            "verdict": (
                "basis_contraction_candidate"
                if len(signals) >= min_signals
                and contractions / len(signals) >= min_reversion_rate
                else "observe_only_insufficient_or_inconsistent_contraction"
            ),
        }
    return {
        "exchanges": by_exchange,
        "total_signals": sum(item["signals"] for item in by_exchange.values()),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--lookback", type=int, default=20)
    parser.add_argument("--horizon", type=int, default=3)
    parser.add_argument("--min-z", type=float, default=2.0)
    parser.add_argument("--min-signals", type=int, default=5)
    parser.add_argument("--min-reversion-rate", type=float, default=0.5)
    options = parser.parse_args()
    if (options.lookback <= 1 or options.horizon <= 0 or options.min_z < 0
            or options.min_signals <= 0 or not 0 <= options.min_reversion_rate <= 1):
        parser.error("invalid lookback, horizon, z-score or reversion thresholds")
    records, invalid_lines = load_records(options.input)
    series = basis_series(records, options.symbol)
    summary = summarize_series(
        series, options.lookback, options.horizon, options.min_z,
        options.min_signals, options.min_reversion_rate,
    )
    print(json.dumps({
        "strategy": "crypto_basis_replay",
        "input": str(options.input),
        "symbol": options.symbol,
        "invalid_lines": invalid_lines,
        "parameters": {
            "lookback": options.lookback,
            "horizon": options.horizon,
            "min_z": options.min_z,
            "min_signals": options.min_signals,
            "min_reversion_rate": options.min_reversion_rate,
        },
        "summary": summary,
        "limitations": [
            "basis snapshots are not synchronized fills or a hedge PnL",
            "fees, borrow, funding transfers, margin, slippage and venue outages are excluded",
            "a short archive cannot establish a stable distribution or causal edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
