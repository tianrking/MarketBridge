#!/usr/bin/env python3
"""Observe put/call option open-interest composition from MarketBridge.

The default Deribit path reports raw contract open interest by option side and
uses a transparent ratio state. Open-interest units and expiry selection remain
provider-specific; this is a research observation, not a hedge or trade signal.
"""

import argparse
import json
import time
from datetime import datetime, timezone

from crypto_microstructure_monitor import fetch


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def payload_row(row):
    payload = row.get("payload") if isinstance(row, dict) else None
    return payload if isinstance(payload, dict) else row if isinstance(row, dict) else {}


def expiry_timestamp(value):
    if not isinstance(value, str):
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def classify_ratio(ratio, high_ratio, low_ratio):
    if ratio is None:
        return "observe_only_missing_put_call_oi"
    if ratio >= high_ratio:
        return "defensive_put_oi"
    if ratio <= low_ratio:
        return "call_dominant_oi"
    return "balanced_oi"


def summarize(rows, currency="BTC", max_expiry_days=180,
             high_ratio=1.0, low_ratio=0.6, now_ts=None):
    now_ts = time.time() if now_ts is None else now_ts
    cutoff = now_ts + max_expiry_days * 86_400.0
    calls = puts = 0.0
    call_contracts = put_contracts = missing_oi = 0
    by_expiry = {}
    for row in rows:
        option = payload_row(row)
        expiry = option.get("expiry_time")
        expiry_ts = expiry_timestamp(expiry)
        if expiry_ts is None or expiry_ts <= now_ts or expiry_ts > cutoff:
            continue
        side = str(option.get("option_type", "")).lower()
        if side not in {"call", "put"}:
            continue
        oi = number(option.get("open_interest"))
        bucket = by_expiry.setdefault(expiry, {"call_oi": 0.0, "put_oi": 0.0,
                                                "call_contracts": 0, "put_contracts": 0,
                                                "missing_oi": 0})
        if oi is None or oi < 0:
            missing_oi += 1
            bucket["missing_oi"] += 1
            continue
        if side == "call":
            calls += oi
            call_contracts += 1
            bucket["call_oi"] += oi
            bucket["call_contracts"] += 1
        else:
            puts += oi
            put_contracts += 1
            bucket["put_oi"] += oi
            bucket["put_contracts"] += 1
    ratio = puts / calls if calls > 0 else None
    total = calls + puts
    state = classify_ratio(ratio, high_ratio, low_ratio)
    expiry_rows = []
    for expiry, bucket in by_expiry.items():
        expiry_total = bucket["call_oi"] + bucket["put_oi"]
        expiry_ratio = bucket["put_oi"] / bucket["call_oi"] if bucket["call_oi"] > 0 else None
        expiry_rows.append({
            "expiry_time": expiry,
            **bucket,
            "put_call_oi_ratio": expiry_ratio,
            "call_share": bucket["call_oi"] / expiry_total if expiry_total else None,
            "state": classify_ratio(expiry_ratio, high_ratio, low_ratio),
        })
    expiry_rows.sort(key=lambda item: expiry_timestamp(item["expiry_time"]) or float("inf"))
    return {
        "currency": currency.upper(),
        "max_expiry_days": max_expiry_days,
        "call_oi": calls,
        "put_oi": puts,
        "total_oi": total,
        "put_call_oi_ratio": ratio,
        "call_share": calls / total if total else None,
        "call_contracts": call_contracts,
        "put_contracts": put_contracts,
        "missing_oi": missing_oi,
        "expiry_count": len(expiry_rows),
        "expiry_rows": expiry_rows,
        "state": state,
        "thresholds": {"high_ratio": high_ratio, "low_ratio": low_ratio},
    }


def observe(base_url, currency, venue, max_expiry_days, high_ratio, low_ratio, timeout):
    payload = fetch(base_url, "/v1/options/chains", {
        "venue": venue, "currency": currency, "include_stale": "false",
    }, timeout)
    result = summarize(payload.get("chains", []), currency, max_expiry_days,
                       high_ratio, low_ratio)
    result.update({
        "venue": venue,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "open-interest units and coverage are provider-specific and may differ across venues",
            "expiry filtering is a snapshot window; contracts roll and are not a historical surface",
            "put/call composition is descriptive and does not infer dealer sign, hedge need or direction",
            "no option spread, margin, transaction cost, hedge or execution model is included",
        ],
        "execution": "research_only_no_orders",
    })
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--max-expiry-days", type=float, default=180.0)
    parser.add_argument("--high-ratio", type=float, default=1.0)
    parser.add_argument("--low-ratio", type=float, default=0.6)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.max_expiry_days <= 0 or args.high_ratio <= args.low_ratio
            or args.low_ratio < 0 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid expiry, ratio, iteration, interval or timeout arguments")
    for iteration in range(args.iterations):
        result = observe(args.base_url, args.currency, args.venue, args.max_expiry_days,
                         args.high_ratio, args.low_ratio, args.timeout)
        print(json.dumps({"strategy": "crypto_options_put_call_oi_monitor",
                          "iteration": iteration + 1, **result},
                         ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
