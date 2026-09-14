#!/usr/bin/env python3
"""Replay BTC response after low-volatility plus downside-skew states.

The input is the JSONL archive produced by
``crypto_options_skew_response_recorder.py``.  This case tests a joint,
observable hypothesis: a quiet trailing quote path together with relatively
expensive put-wing IV may be followed by a different BTC response than
one-dimensional or ordinary states.  The volatility measure is deliberately
an unannualized per-record log-return standard deviation because recorder
cadence is caller-controlled.
"""

import argparse
import json
import math
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
    if isinstance(value, bool):
        return None
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    return parsed if math.isfinite(parsed) else None


def quote_price(record):
    return number((((record.get("observation") or {}).get("price") or {}).get("price")))


def realized_volatility_proxy_pct(prices):
    """Return per-record log-return volatility in percentage points."""

    if len(prices) < 3 or any(price is None or price <= 0 for price in prices):
        return None
    returns = [math.log(prices[index] / prices[index - 1])
               for index in range(1, len(prices))]
    return statistics.pstdev(returns) * 100.0


def skew_vol_state(realized_vol_pct, skew_iv, low_vol_pct, downside_skew_iv):
    if realized_vol_pct is None or skew_iv is None:
        return "observe_only_missing_vol_or_skew"
    low_vol = realized_vol_pct <= low_vol_pct
    downside = skew_iv >= downside_skew_iv
    if low_vol and downside:
        return "low_vol_downside_skew"
    if low_vol:
        return "low_vol_without_downside_skew"
    if downside:
        return "downside_skew_without_low_vol"
    return "ordinary_vol_skew"


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [abs(value) for value in returns]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)
                              if returns else None),
    }


def summarize_records(records, horizon_records, vol_window, low_vol_pct,
                      downside_skew_iv, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    timestamps = [item.get("recorded_at_ms", 0) for item in ordered]
    observations = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            continue
        current = quote_price(record)
        future = quote_price(ordered[future_index])
        if current is None or future is None or current <= 0 or future <= 0:
            continue
        prior_prices = [quote_price(ordered[prior])
                        for prior in range(max(0, index - vol_window), index + 1)]
        realized_vol_pct = realized_volatility_proxy_pct(prior_prices)
        target = ((record.get("observation") or {}).get("target_expiry") or {})
        skew_iv = number(target.get("put_call_skew_iv"))
        observations.append({
            "recorded_at_ms": timestamps[index],
            "state": skew_vol_state(realized_vol_pct, skew_iv, low_vol_pct,
                                     downside_skew_iv),
            "realized_volatility_proxy_pct": realized_vol_pct,
            "put_call_skew_iv": skew_iv,
            "forward_return_pct": (future / current - 1.0) * 100.0,
        })
    states = ("low_vol_downside_skew", "low_vol_without_downside_skew",
              "downside_skew_without_low_vol", "ordinary_vol_skew",
              "observe_only_missing_vol_or_skew")
    by_state = {state: bucket_stats([row for row in observations
                                     if row["state"] == state]) for state in states}
    valid = len(observations) - by_state["observe_only_missing_vol_or_skew"]["observations"]
    joint = by_state["low_vol_downside_skew"]["observations"]
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "valid_vol_skew_windows": valid,
        "low_vol_downside_skew_windows": joint,
        "verdict": ("options_low_vol_downside_skew_response_reported"
                     if joint >= min_observations
                     else "observe_only_insufficient_joint_low_vol_downside_skew_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--vol-window", type=int, default=6,
                        help="prior records used for the volatility proxy")
    parser.add_argument("--low-vol-pct", type=float, default=1.0,
                        help="maximum per-record log-return volatility in percent")
    parser.add_argument("--downside-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or args.vol_window < 2 or args.low_vol_pct < 0
            or args.downside_skew_iv < 0 or args.min_observations <= 0):
        parser.error("invalid horizon, volatility window, thresholds or minimum observations")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_options_skew_vol_regime_response_replay",
        "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "vol_window": args.vol_window, "low_vol_pct": args.low_vol_pct,
                    "downside_skew_iv": args.downside_skew_iv,
                    "min_observations": args.min_observations},
        "volatility_convention": "pstdev(log(price_t / price_t-1)) * 100; unannualized per-record proxy",
        "skew_convention": "put_call_skew_iv = put_iv_minus_call_iv; positive means higher put-wing IV",
        "summary": summarize_records(records, args.horizon_records, args.vol_window,
                                      args.low_vol_pct, args.downside_skew_iv,
                                      args.min_observations),
        "limitations": [
            "the volatility proxy depends on recorder cadence and is not annualized realized volatility",
            "ATM IV and moneyness-bucket skew are venue snapshots, not executable option prices",
            "expiry identity can roll; no dealer sign, option PnL, hedge, margin, fees or execution are modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
