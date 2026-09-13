#!/usr/bin/env python3
"""Measure price calibration against a resolved Polymarket outcome.

Each BUY trade is treated as a probability estimate for the purchased outcome:
price is the implied probability that one share pays one dollar. This report
is descriptive and intentionally does not copy wallets or place trades.
"""

import argparse
import json
import math
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trades-jsonl", type=Path, required=True)
    parser.add_argument("--resolved-outcome", required=True)
    parser.add_argument("--bins", type=int, default=10)
    options = parser.parse_args()
    if not 2 <= options.bins <= 20:
        parser.error("--bins must be between 2 and 20")

    rows = []
    with options.trades_jsonl.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                raise SystemExit(f"invalid JSONL at {options.trades_jsonl}:{line_number}: {error}") from error
            if str(row.get("side") or "").upper() != "BUY":
                continue
            price = row.get("price")
            if isinstance(price, (int, float)) and 0.0 <= price <= 1.0:
                rows.append(row)
    if not rows:
        raise SystemExit("no valid BUY rows in the supplied JSONL")

    brier = 0.0
    log_loss = 0.0
    bins = [{"count": 0, "sum_price": 0.0, "wins": 0} for _ in range(options.bins)]
    for row in rows:
        price = float(row["price"])
        won = row.get("outcome") == options.resolved_outcome
        target = 1.0 if won else 0.0
        brier += (price - target) ** 2
        safe_price = min(max(price, 1e-9), 1.0 - 1e-9)
        log_loss -= target * math.log(safe_price)
        log_loss -= (1.0 - target) * math.log(1.0 - safe_price)
        index = min(int(price * options.bins), options.bins - 1)
        bucket = bins[index]
        bucket["count"] += 1
        bucket["sum_price"] += price
        bucket["wins"] += int(won)

    calibration = []
    for index, bucket in enumerate(bins):
        if bucket["count"] == 0:
            continue
        calibration.append({
            "lower": index / options.bins,
            "upper": (index + 1) / options.bins,
            "count": bucket["count"],
            "mean_implied_probability": bucket["sum_price"] / bucket["count"],
            "empirical_win_rate": bucket["wins"] / bucket["count"],
        })
    print(json.dumps({
        "rows": len(rows),
        "resolved_outcome": options.resolved_outcome,
        "brier_score": brier / len(rows),
        "log_loss": log_loss / len(rows),
        "calibration": calibration,
        "limitations": [
            "trade observations are not a complete participant fill ledger",
            "selection bias and repeated wallet behavior are not removed",
            "calibration is descriptive and not a trading recommendation",
        ],
    }, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
