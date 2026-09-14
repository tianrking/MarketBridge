#!/usr/bin/env python3
"""Replay BTC response after joint option-IV panic regimes.

The recorder input already contains a target expiry's ATM IV and
``put_call_skew_iv``.  This replay tests whether high ATM IV combined with
downside skew has a different later BTC response than one-dimensional or
ordinary states.  MarketBridge defines skew as put IV minus call IV, so a
positive value means relatively higher put-wing IV in this case.
"""

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


def panic_state(target, high_atm_iv, downside_skew_iv):
    target = target or {}
    atm_iv = number(target.get("atm_iv"))
    skew = number(target.get("put_call_skew_iv"))
    if atm_iv is None or skew is None:
        return "observe_only_missing_iv_or_skew"
    high_iv = atm_iv >= high_atm_iv
    downside = skew >= downside_skew_iv
    if high_iv and downside:
        return "high_iv_downside_skew"
    if high_iv:
        return "high_iv_without_downside_skew"
    if downside:
        return "downside_skew_without_high_iv"
    return "ordinary_iv_skew"


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [abs(value) for value in returns]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "negative_forward_fraction": (sum(value < 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize_records(records, horizon_records, high_atm_iv, downside_skew_iv, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    observations = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            continue
        current = number((((record.get("observation") or {}).get("price") or {}).get("price")))
        future = number((((ordered[future_index].get("observation") or {}).get("price") or {}).get("price")))
        if current is None or future is None or current <= 0 or future <= 0:
            continue
        target = (record.get("observation") or {}).get("target_expiry") or {}
        observations.append({
            "recorded_at_ms": record.get("recorded_at_ms"),
            "state": panic_state(target, high_atm_iv, downside_skew_iv),
            "atm_iv": target.get("atm_iv"),
            "put_call_skew_iv": target.get("put_call_skew_iv"),
            "forward_return_pct": (future / current - 1.0) * 100.0,
        })
    states = ("high_iv_downside_skew", "high_iv_without_downside_skew",
              "downside_skew_without_high_iv", "ordinary_iv_skew",
              "observe_only_missing_iv_or_skew")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    valid = len(observations) - by_state["observe_only_missing_iv_or_skew"]["observations"]
    panic = by_state["high_iv_downside_skew"]["observations"]
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "valid_iv_skew_windows": valid,
        "high_iv_downside_skew_windows": panic,
        "verdict": ("options_panic_regime_response_reported"
                     if panic >= min_observations
                     else "observe_only_insufficient_joint_panic_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--high-atm-iv", type=float, default=60.0)
    parser.add_argument("--downside-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or args.high_atm_iv < 0
            or args.downside_skew_iv < 0 or args.min_observations <= 0):
        parser.error("invalid horizon, IV thresholds or minimum observations")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_options_panic_regime_response_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records, "high_atm_iv": args.high_atm_iv,
                    "downside_skew_iv": args.downside_skew_iv,
                    "min_observations": args.min_observations},
        "skew_convention": "put_call_skew_iv = put_iv_minus_call_iv; positive means higher put-wing IV",
        "summary": summarize_records(records, args.horizon_records, args.high_atm_iv,
                                      args.downside_skew_iv, args.min_observations),
        "limitations": [
            "ATM IV and moneyness-bucket skew are venue snapshots, not executable option prices",
            "expiry identity can roll and the IV threshold is a caller-supplied regime label",
            "joint states are descriptive; no dealer sign, option PnL, hedge, margin, fees or execution are modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
