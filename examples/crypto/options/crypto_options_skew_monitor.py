#!/usr/bin/env python3
"""Observe BTC/ETH option IV skew and term structure from MarketBridge.

The public options narrative is reduced to measurable snapshot hypotheses:
downside puts may trade at a higher implied volatility than comparable calls,
and near-term ATM IV may differ from the next expiry.  Moneyness buckets are
the compatibility default; an explicit delta mode uses provider greeks when a
venue exposes them.  It never prices a trade or places an order.
"""

import argparse
import json
import statistics
import time
from datetime import datetime, timezone

from crypto_microstructure_monitor import fetch


def number(value):
    return float(value) if isinstance(value, (int, float)) else None


def payload_row(row):
    """Unwrap a v1 DataEnvelope while tolerating already-flat fixtures."""
    payload = row.get("payload") if isinstance(row, dict) else None
    return payload if isinstance(payload, dict) else row if isinstance(row, dict) else {}


def expiry_timestamp(expiry_time):
    if not isinstance(expiry_time, str):
        return None
    try:
        return datetime.fromisoformat(expiry_time.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def bucket_rows(rows, underlying, atm_band, wing_min, wing_max,
                bucket_mode="moneyness", delta_band=0.05):
    result = {"atm": [], "put_wing": [], "call_wing": []}
    if underlying is None or underlying <= 0:
        return result
    for row in rows:
        mark_iv = number(row.get("mark_iv"))
        if mark_iv is None or mark_iv <= 0:
            continue
        option_type = str(row.get("option_type", "")).lower()
        if bucket_mode == "delta":
            delta = number(row.get("delta"))
            if delta is None or not -1.0 <= delta <= 1.0:
                continue
            if abs(abs(delta) - 0.5) <= delta_band:
                result["atm"].append(mark_iv)
            elif option_type == "put" and abs(delta + 0.25) <= delta_band:
                result["put_wing"].append(mark_iv)
            elif option_type == "call" and abs(delta - 0.25) <= delta_band:
                result["call_wing"].append(mark_iv)
            continue
        strike = number(row.get("strike"))
        if strike is None or strike <= 0:
            continue
        moneyness = strike / underlying
        if abs(moneyness - 1.0) <= atm_band:
            result["atm"].append(mark_iv)
        elif option_type == "put" and wing_min <= moneyness < 1.0:
            result["put_wing"].append(mark_iv)
        elif option_type == "call" and 1.0 < moneyness <= wing_max:
            result["call_wing"].append(mark_iv)
    return result


def median_or_none(values):
    return statistics.median(values) if values else None


def summarize_expiry(rows, expiry, now_ts, atm_band, wing_min, wing_max,
                     bucket_mode="moneyness", delta_band=0.05):
    payloads = [payload_row(row) for row in rows]
    underlyings = [number(row.get("underlying_price")) for row in payloads]
    underlying = median_or_none([value for value in underlyings if value is not None and value > 0])
    buckets = bucket_rows(payloads, underlying, atm_band, wing_min, wing_max,
                          bucket_mode, delta_band)
    atm_iv = median_or_none(buckets["atm"])
    put_iv = median_or_none(buckets["put_wing"])
    call_iv = median_or_none(buckets["call_wing"])
    return {
        "expiry_time": expiry,
        "bucket_mode": bucket_mode,
        "days_to_expiry": ((expiry_timestamp(expiry) - now_ts) / 86_400.0) if expiry_timestamp(expiry) else None,
        "underlying_price": underlying,
        "contracts": len(payloads),
        "atm_contracts": len(buckets["atm"]),
        "put_wing_contracts": len(buckets["put_wing"]),
        "call_wing_contracts": len(buckets["call_wing"]),
        "atm_iv": atm_iv,
        "put_wing_iv": put_iv,
        "call_wing_iv": call_iv,
        "put_call_skew_iv": put_iv - call_iv if put_iv is not None and call_iv is not None else None,
    }


def classify_skew(summary, min_skew_iv, max_skew_iv):
    skew = summary.get("put_call_skew_iv")
    if skew is None:
        return "observe_only_missing_comparable_wings"
    if skew >= min_skew_iv:
        return "downside_protection_demand"
    if skew <= -max_skew_iv:
        return "upside_call_demand"
    return "balanced_wing_iv"


def classify_term_structure(near_atm_iv, far_atm_iv, min_slope_iv):
    if near_atm_iv is None or far_atm_iv is None:
        return "observe_only_missing_term_points"
    slope = far_atm_iv - near_atm_iv
    if slope >= min_slope_iv:
        return "upward_iv_term_structure"
    if slope <= -min_slope_iv:
        return "inverted_iv_term_structure"
    return "flat_iv_term_structure"


def observe(base_url, currency, venue, expiry_days, atm_band, wing_min, wing_max,
            min_skew_iv, min_term_slope_iv, timeout,
            bucket_mode="moneyness", delta_band=0.05):
    payload = fetch(base_url, "/v1/options/chains", {
        "venue": venue,
        "currency": currency,
        "include_stale": "false",
    }, timeout)
    now_ts = time.time()
    grouped = {}
    for row in payload.get("chains", []):
        option = payload_row(row)
        expiry = option.get("expiry_time")
        expiry_ts = expiry_timestamp(expiry)
        if expiry_ts is None or expiry_ts <= now_ts:
            continue
        grouped.setdefault(expiry, []).append(row)
    summaries = [summarize_expiry(rows, expiry, now_ts, atm_band, wing_min, wing_max,
                                  bucket_mode, delta_band)
                 for expiry, rows in grouped.items()]
    summaries.sort(key=lambda row: row.get("days_to_expiry") or float("inf"))
    target = min(summaries, key=lambda row: abs((row["days_to_expiry"] or 0) - expiry_days)) if summaries else None
    near = summaries[0] if summaries else None
    far = summaries[1] if len(summaries) > 1 else None
    term_slope = classify_term_structure(
        near.get("atm_iv") if near else None,
        far.get("atm_iv") if far else None,
        min_term_slope_iv,
    )
    if target:
        target["skew_state"] = classify_skew(target, min_skew_iv, min_skew_iv)
    return {
        "currency": currency.upper(),
        "venue": venue,
        "target_expiry": target,
        "term_structure": {
            "near_expiry": near,
            "far_expiry": far,
            "atm_iv_slope_iv": (far["atm_iv"] - near["atm_iv"])
            if near and far and near.get("atm_iv") is not None and far.get("atm_iv") is not None else None,
            "state": term_slope,
        },
        "available_expiries": len(summaries),
        "stale_rows_included": False,
        "upstream_errors": payload.get("errors", []),
        "evidence": [
            "option_chain_available" if summaries else "missing_option_chain",
            (f"comparable_{bucket_mode}_buckets_available"
             if target and target.get("put_call_skew_iv") is not None
             else f"missing_comparable_{bucket_mode}_buckets"),
            "two_expiry_term_point_available" if far else "missing_second_expiry",
        ],
        "limitations": [
            "delta mode requires comparable provider delta conventions and skips contracts without delta",
            "moneyness mode is a transparent bucket proxy, not a universal 25-delta surface",
            "mark IV and open interest are venue snapshots, not executable quotes or historical surfaces",
            "no option spread, margin, hedge, transaction cost or volatility-surface interpolation model",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--atm-band", type=float, default=0.03)
    parser.add_argument("--wing-min", type=float, default=0.85)
    parser.add_argument("--wing-max", type=float, default=1.15)
    parser.add_argument("--bucket-mode", choices=("moneyness", "delta"), default="moneyness",
                        help="wing buckets: strike moneyness (default) or provider delta")
    parser.add_argument("--delta-band", type=float, default=0.05,
                        help="absolute delta tolerance for ATM/25-delta buckets")
    parser.add_argument("--min-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-term-slope-iv", type=float, default=3.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.expiry_days <= 0 or not 0 < options.atm_band < 0.25
            or not 0 < options.wing_min < 1 or options.wing_max <= 1
            or options.wing_min >= options.wing_max or not 0 < options.delta_band <= 0.25
            or options.min_skew_iv < 0
            or options.min_term_slope_iv < 0 or options.iterations <= 0
            or options.interval_secs < 0):
        parser.error("invalid expiry, moneyness, threshold, iteration or interval arguments")
    for iteration in range(options.iterations):
        result = observe(options.base_url, options.currency, options.venue, options.expiry_days,
                         options.atm_band, options.wing_min, options.wing_max,
                         options.min_skew_iv, options.min_term_slope_iv, options.timeout,
                         options.bucket_mode, options.delta_band)
        print(json.dumps({"strategy": "crypto_options_skew_monitor",
                          "iteration": iteration + 1, **result}, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
