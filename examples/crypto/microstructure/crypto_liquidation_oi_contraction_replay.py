#!/usr/bin/env python3
"""Summarize the corrected contraction-vs-limited OI event study.

Consumes the corrected v3 event table from the public Hyperliquid replication
package. It reports descriptive class summaries only. It does not identify
forced versus voluntary flow, estimate the paper's clustered inference, or
create an execution signal.

Accepted input is CSV, JSON, or JSONL containing ``klass`` and corrected v3
fields such as ``disp``, ``doi``, ``p6h`` and ``p24h``.
Export the source package's Parquet table with its documented pipeline first.
"""

import argparse
import csv
import json
import statistics
from pathlib import Path


CLASS_ALIASES = {
    "contraction": "contraction",
    "limited": "limited",
    "intermediate": "intermediate",
}


def number(value):
    if isinstance(value, bool) or value in (None, ""):
        return None
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    return parsed if parsed == parsed else None


def first(row, *keys):
    for key in keys:
        if row.get(key) not in (None, ""):
            return row[key]
    return None


def normalize(row):
    raw_class = str(first(row, "klass", "class", "event_class", "mechanism") or "").lower()
    klass = CLASS_ALIASES.get(raw_class, "unknown")
    return {
        "klass": klass,
        "doi": number(first(row, "doi")),
        "notional_w": number(first(row, "notional_W", "notional_w")),
        "disp": number(first(row, "disp")),
        "p6h": number(first(row, "p6h")),
        "p24h": number(first(row, "p24h")),
    }


def load_rows(path):
    suffix = path.suffix.lower()
    if suffix == ".csv":
        with path.open(encoding="utf-8", newline="") as handle:
            return [normalize(row) for row in csv.DictReader(handle)]
    text = path.read_text(encoding="utf-8")
    if suffix == ".json":
        payload = json.loads(text)
        if isinstance(payload, dict):
            payload = payload.get("rows", payload.get("events", []))
        return [normalize(row) for row in payload if isinstance(row, dict)]
    rows = []
    for line in text.splitlines():
        if line.strip():
            value = json.loads(line)
            if isinstance(value, dict):
                rows.append(normalize(value))
    return rows


def values(rows, field, absolute=False):
    result = [row[field] for row in rows if row.get(field) is not None]
    return [abs(value) for value in result] if absolute else result


def median_or_none(items):
    return statistics.median(items) if items else None


def group_summary(rows):
    summary = {}
    for klass in ("contraction", "limited", "intermediate", "unknown"):
        group = [row for row in rows if row["klass"] == klass]
        oi_changes = values(group, "doi")
        displacements = values(group, "disp", absolute=True)
        returns_6h = values(group, "p6h")
        returns_24h = values(group, "p24h")
        summary[klass] = {
            "observations": len(group),
            "median_oi_change_pct": median_or_none(oi_changes) * 100 if oi_changes else None,
            "median_liquidation_notional_window_usd": median_or_none(values(group, "notional_w")),
            "median_abs_displacement_pct": median_or_none(displacements) * 100 if displacements else None,
            "median_forward_6h_pct": median_or_none(returns_6h) * 100 if returns_6h else None,
            "median_forward_24h_pct": median_or_none(returns_24h) * 100 if returns_24h else None,
        }
    return summary


def summarize(rows, min_observations):
    groups = {klass: [row for row in rows if row["klass"] == klass]
              for klass in ("contraction", "limited")}
    medians = {
        klass: median_or_none(values(groups[klass], "disp", absolute=True))
        for klass in groups
    }
    enough = all(len(groups[klass]) >= min_observations for klass in groups)
    return {
        "by_class": group_summary(rows),
        "contraction_minus_limited": {
            "median_abs_displacement_pct": 100 * (medians["contraction"] - medians["limited"])
            if medians["contraction"] is not None and medians["limited"] is not None else None,
        },
        "min_observations": min_observations,
        "verdict": "corrected_v3_descriptive_summary" if enough
        else "observe_only_insufficient_class_sample",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True,
                        help="CSV, JSON, or JSONL derived event table")
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.min_observations <= 0:
        parser.error("min-observations must be positive")
    rows = load_rows(args.input)
    print(json.dumps({
        "strategy": "crypto_liquidation_oi_contraction_replay",
        "hypothesis": (
            "OI-contraction and limited-contraction classes among liquidation-triggered "
            "events may differ in displacement and forward price response; this "
            "descriptive consumer does not identify the closing mechanism"
        ),
        "input": str(args.input),
        "source_schema": {
            "required": ["klass"],
            "corrected_v3_fields": ["doi", "notional_W", "disp", "p6h", "p24h"],
            "class_semantics": "contraction / limited / intermediate OI response among liquidation-triggered events",
        },
        "source_counts": {"rows": len(rows), "unknown_class": sum(row["klass"] == "unknown" for row in rows)},
        "summary": summarize(rows, args.min_observations),
        "provenance": {
            "study": "Open Interest Contraction and Price Recovery Around Liquidation Events (corrected v3)",
            "replication": "https://github.com/edwinyeeshunwan/forced-or-frantic",
            "venue": "hyperliquid",
            "source_version": "v3 corrected September 2026",
        },
        "limitations": [
            "v1 was withdrawn after event-clock, OI-staleness and inference errors; only the corrected v3 outputs should be used",
            "the class label describes aggregate OI contraction within liquidation-triggered events, not verified forced versus voluntary flow",
            "this consumer reports descriptive medians only; it does not reproduce the source's episode-clustered uncertainty estimates",
            "one venue, BTC and ETH, September-December 2025; exploratory and not prospectively validated",
            "MarketBridge does not currently reconstruct the source archive's full Hyperliquid event table",
            "no signal, position, funding cash flow, fees, slippage or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
