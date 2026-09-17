#!/usr/bin/env python3
"""Compare forced-deleveraging and voluntary-churn liquidation events.

This is a small consumer for the derived event table published with the
"Forced or Frantic?" replication package.  The source table already applies
the study's pre-registered Hyperliquid liquidation/OI classification; this
script keeps the class label and outcome fields auditable instead of treating
an ordinary venue liquidation feed as complete.  It reports descriptive
group differences only and never creates an execution signal.

Accepted input is CSV, JSON, or JSONL with at least ``klass`` and one outcome
field (for example ``peak_disloc``).  The source package's Parquet table can
be exported to CSV/JSON with its own documented pipeline before running this
example; no parquet dependency is required here.
"""

import argparse
import csv
import json
import random
import statistics
from pathlib import Path


CLASS_ALIASES = {
    "deleverage": "deleverage",
    "forced": "deleverage",
    "forced_deleveraging": "deleverage",
    "churn": "churn",
    "voluntary": "churn",
    "voluntary_churn": "churn",
    "middle": "middle",
}


def number(value):
    if isinstance(value, bool) or value in (None, ""):
        return None
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    return parsed if parsed == parsed else None


def boolean(value):
    if isinstance(value, bool):
        return value
    if value is None:
        return None
    return str(value).strip().lower() in {"1", "true", "yes", "y"}


def first(row, *keys):
    for key in keys:
        if row.get(key) not in (None, ""):
            return row[key]
    return None


def normalize(row):
    raw_class = str(first(row, "klass", "class", "event_class", "mechanism") or "").lower()
    klass = CLASS_ALIASES.get(raw_class, "unknown")
    return {
        "t0": first(row, "t0", "ts_ms", "event_time_ms", "timestamp"),
        "klass": klass,
        "liq_notional": number(first(row, "liq_notional", "liquidation_notional", "notional")),
        "peak_disloc": number(first(row, "peak_disloc", "peak_dislocation", "peak_dislocation_log_return")),
        "transitory_share": number(first(row, "transitory_share", "transitory_fraction")),
        "ttr_min": number(first(row, "ttr_min", "time_to_recovery_min", "recovery_minutes")),
        "censored": boolean(first(row, "censored", "recovery_censored")),
        "perm_6h": number(first(row, "perm_6h", "permanent_6h")),
        "perm_24h": number(first(row, "perm_24h", "permanent_24h")),
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
    result = [row[field] for row in rows if row[field] is not None]
    return [abs(value) for value in result] if absolute else result


def median_or_none(items):
    return statistics.median(items) if items else None


def permutation_pvalue(left, right, permutations, seed):
    if len(left) < 2 or len(right) < 2:
        return None
    observed = statistics.median(left) - statistics.median(right)
    pool = list(left) + list(right)
    size = len(left)
    rng = random.Random(seed)
    exceed = 0
    for _ in range(permutations):
        shuffled = pool[:]
        rng.shuffle(shuffled)
        trial = statistics.median(shuffled[:size]) - statistics.median(shuffled[size:])
        if abs(trial) >= abs(observed):
            exceed += 1
    return (exceed + 1) / (permutations + 1)


def group_summary(rows):
    summary = {}
    for klass in ("deleverage", "churn", "middle", "unknown"):
        group = [row for row in rows if row["klass"] == klass]
        ttr = values(group, "ttr_min")
        censored = [row["censored"] for row in group if row["censored"] is not None]
        summary[klass] = {
            "observations": len(group),
            "median_peak_abs_disloc": median_or_none(values(group, "peak_disloc", True)),
            "median_transitory_share": median_or_none(values(group, "transitory_share")),
            "median_ttr_min": median_or_none(ttr),
            "censored_fraction": (sum(censored) / len(censored) if censored else None),
        }
    return summary


def summarize(rows, min_observations, permutations, seed):
    groups = {klass: [row for row in rows if row["klass"] == klass]
              for klass in ("deleverage", "churn")}
    peak = {klass: values(groups[klass], "peak_disloc", True) for klass in groups}
    transitory = {klass: values(groups[klass], "transitory_share") for klass in groups}
    peak_edge = (median_or_none(peak["deleverage"]) - median_or_none(peak["churn"])
                 if peak["deleverage"] and peak["churn"] else None)
    transitory_edge = (median_or_none(transitory["deleverage"])
                       - median_or_none(transitory["churn"])
                       if transitory["deleverage"] and transitory["churn"] else None)
    enough = all(len(groups[klass]) >= min_observations for klass in groups)
    return {
        "by_class": group_summary(rows),
        "deleverage_minus_churn": {
            "median_peak_abs_disloc": peak_edge,
            "peak_abs_disloc_permutation_p": permutation_pvalue(
                peak["deleverage"], peak["churn"], permutations, seed),
            "median_transitory_share": transitory_edge,
            "transitory_share_permutation_p": permutation_pvalue(
                transitory["deleverage"], transitory["churn"], permutations, seed + 1),
        },
        "min_observations": min_observations,
        "permutations": permutations,
        "verdict": "forced_vs_voluntary_response_reported" if enough
        else "observe_only_insufficient_class_sample",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True,
                        help="CSV, JSON, or JSONL derived event table")
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--permutations", type=int, default=2000)
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args()
    if args.min_observations <= 0 or args.permutations <= 0:
        parser.error("min-observations and permutations must be positive")
    rows = load_rows(args.input)
    print(json.dumps({
        "strategy": "crypto_forced_vs_voluntary_liquidation_replay",
        "hypothesis": "forced-deleveraging and voluntary-churn events of comparable observed liquidation context may have different price dislocation and recovery responses",
        "input": str(args.input),
        "source_schema": {
            "required": ["klass"],
            "outcomes": ["peak_disloc", "transitory_share", "ttr_min", "censored"],
            "class_semantics": "deleverage/forced versus churn/voluntary; middle is retained as context",
        },
        "source_counts": {"rows": len(rows), "unknown_class": sum(row["klass"] == "unknown" for row in rows)},
        "summary": summarize(rows, args.min_observations, args.permutations, args.seed),
        "provenance": {
            "study": "Forced or Frantic? — Deleveraging Cascades and Price Dislocation on a Transparent Perpetual-Futures Venue",
            "replication": "https://github.com/edwinyeeshunwan/forced-or-frantic",
            "venue": "hyperliquid",
        },
        "limitations": [
            "the class label and outcomes come from the supplied derived event table; this script does not infer forced versus voluntary from an incomplete CEX feed",
            "Hyperliquid complete liquidation history is not currently a MarketBridge historical-liquidations provider; source acquisition remains a required data feature",
            "permutation p-values are descriptive and do not adjust for event dependence or multiple research choices",
            "no signal, position, funding cash flow, fees, slippage or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
