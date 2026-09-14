#!/usr/bin/env python3
"""Observe expiry-level max-pain proxies from MarketBridge option chains.

The calculation uses provider open interest and a transparent intrinsic-pain
proxy for each candidate settlement strike. It is most informative near an
expiry with substantial OI, but it does not imply that price will be pinned or
that any participant can move the market.
"""

import argparse
import json
import statistics
import time

from crypto_microstructure_monitor import fetch
from crypto_options_put_call_oi_monitor import expiry_timestamp, number, payload_row


def pain_proxy(candidate, option_rows):
    """Return relative intrinsic payout proxy for one candidate settlement.

    Deribit BTC option OI is reported in base-asset units. Dividing the
    intrinsic USD amount by candidate settlement keeps the proxy in base-asset
    units while preserving strike ranking for inverse-style BTC options.
    """
    if candidate <= 0:
        return None
    total = 0.0
    for row in option_rows:
        strike, oi = row["strike"], row["open_interest"]
        if row["option_type"] == "call":
            intrinsic = max(candidate - strike, 0.0)
        else:
            intrinsic = max(strike - candidate, 0.0)
        total += intrinsic * oi / candidate
    return total


def classify_expiry(days_to_expiry, distance_pct, oi_total,
                    near_expiry_days, near_distance_pct, min_oi):
    if days_to_expiry is None or distance_pct is None or oi_total < min_oi:
        return "observe_only_insufficient_max_pain_inputs"
    if days_to_expiry <= near_expiry_days and abs(distance_pct) <= near_distance_pct:
        return "near_expiry_near_max_pain"
    if days_to_expiry <= near_expiry_days:
        return "near_expiry_far_from_max_pain"
    return "far_expiry_max_pain_context"


def summarize_expiry(rows, expiry, now_ts, near_expiry_days,
                     near_distance_pct, min_oi):
    option_rows = []
    underlying_values = []
    missing = 0
    for row in rows:
        option = payload_row(row)
        side = str(option.get("option_type", "")).lower()
        strike = number(option.get("strike"))
        oi = number(option.get("open_interest"))
        underlying = number(option.get("underlying_price"))
        if underlying is not None and underlying > 0:
            underlying_values.append(underlying)
        if side not in {"call", "put"} or strike is None or strike <= 0 or oi is None or oi < 0:
            missing += 1
            continue
        option_rows.append({"option_type": side, "strike": strike, "open_interest": oi})
    strikes = sorted({row["strike"] for row in option_rows})
    if not strikes:
        return {
            "expiry_time": expiry, "option_rows": 0, "missing_rows": missing,
            "max_pain_strike": None, "underlying_price": None,
            "days_to_expiry": None, "distance_to_max_pain_pct": None,
            "open_interest": 0.0, "state": "observe_only_insufficient_max_pain_inputs",
        }
    pain_by_strike = {strike: pain_proxy(strike, option_rows) for strike in strikes}
    max_pain = min(pain_by_strike, key=lambda strike: (pain_by_strike[strike], strike))
    underlying = statistics.median(underlying_values) if underlying_values else None
    expiry_ts = expiry_timestamp(expiry)
    days = (expiry_ts - now_ts) / 86_400.0 if expiry_ts else None
    distance = ((max_pain / underlying) - 1.0) * 100.0 if underlying else None
    oi_total = sum(row["open_interest"] for row in option_rows)
    return {
        "expiry_time": expiry,
        "option_rows": len(option_rows),
        "missing_rows": missing,
        "strike_count": len(strikes),
        "max_pain_strike": max_pain,
        "underlying_price": underlying,
        "days_to_expiry": days,
        "distance_to_max_pain_pct": distance,
        "open_interest": oi_total,
        "pain_proxy_at_max": pain_by_strike[max_pain],
        "state": classify_expiry(days, distance, oi_total, near_expiry_days,
                                  near_distance_pct, min_oi),
    }


def summarize(rows, currency="BTC", max_expiry_days=180.0,
             near_expiry_days=3.0, near_distance_pct=2.0, min_oi=0.0,
             now_ts=None):
    now_ts = time.time() if now_ts is None else now_ts
    cutoff = now_ts + max_expiry_days * 86_400.0
    grouped = {}
    for row in rows:
        option = payload_row(row)
        expiry = option.get("expiry_time")
        expiry_ts = expiry_timestamp(expiry)
        if expiry_ts is None or expiry_ts <= now_ts or expiry_ts > cutoff:
            continue
        grouped.setdefault(expiry, []).append(row)
    expiry_rows = [summarize_expiry(group, expiry, now_ts, near_expiry_days,
                                     near_distance_pct, min_oi)
                   for expiry, group in grouped.items()]
    expiry_rows.sort(key=lambda row: expiry_timestamp(row["expiry_time"]) or float("inf"))
    target = min(expiry_rows, key=lambda row: row.get("days_to_expiry") or float("inf")) if expiry_rows else None
    return {
        "currency": currency.upper(),
        "max_expiry_days": max_expiry_days,
        "near_expiry_days": near_expiry_days,
        "near_distance_pct": near_distance_pct,
        "min_oi": min_oi,
        "expiry_count": len(expiry_rows),
        "target_expiry": target,
        "expiry_rows": expiry_rows,
        "evidence": [
            "max_pain_proxy_available" if target and target.get("max_pain_strike") is not None
            else "missing_max_pain_inputs",
            "near_expiry_window_present" if any(row["days_to_expiry"] is not None
                                                and row["days_to_expiry"] <= near_expiry_days
                                                for row in expiry_rows)
            else "no_near_expiry_window",
        ],
    }


def observe(base_url, currency, venue, max_expiry_days, near_expiry_days,
            near_distance_pct, min_oi, timeout):
    payload = fetch(base_url, "/v1/options/chains", {
        "venue": venue, "currency": currency, "include_stale": "false",
    }, timeout)
    result = summarize(payload.get("chains", []), currency, max_expiry_days,
                       near_expiry_days, near_distance_pct, min_oi)
    result.update({
        "venue": venue,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "the pain value is a provider-unit intrinsic proxy, not a complete option PnL ledger",
            "max pain is descriptive and does not prove price pinning, intent or dealer positioning",
            "Deribit settlement uses an index TWAP and expiry/contract coverage can roll",
            "no order, hedge, margin, cost, wallet or execution model is included",
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
    parser.add_argument("--near-expiry-days", type=float, default=3.0)
    parser.add_argument("--near-distance-pct", type=float, default=2.0)
    parser.add_argument("--min-oi", type=float, default=0.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.max_expiry_days <= 0 or args.near_expiry_days < 0
            or args.near_expiry_days > args.max_expiry_days or args.near_distance_pct < 0
            or args.min_oi < 0 or args.iterations <= 0 or args.interval_secs < 0
            or args.timeout <= 0):
        parser.error("invalid expiry, distance, OI, iteration, interval or timeout arguments")
    for iteration in range(args.iterations):
        result = observe(args.base_url, args.currency, args.venue, args.max_expiry_days,
                         args.near_expiry_days, args.near_distance_pct, args.min_oi,
                         args.timeout)
        print(json.dumps({"strategy": "crypto_options_max_pain_monitor",
                          "iteration": iteration + 1, **result},
                         ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
