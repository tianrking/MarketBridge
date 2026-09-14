#!/usr/bin/env python3
"""Map crypto option gamma concentration from MarketBridge chain snapshots.

Public discussions often label a strike as a gamma wall or gamma flip.  This
observer keeps the measurable part and rejects the unsafe shortcut: public
open interest does not reveal whether dealers are long or short gamma.  It
therefore reports unsigned gamma mass, near-spot concentration and strike
locations, never a directional dealer-hedging signal.

The relative mass proxy is ``abs(gamma * open_interest * underlying_price**2)``.
It is useful for comparing strikes inside one snapshot, but is not a USD PnL,
hedge ratio or executable exposure measure because contract multipliers and
position ownership are venue-specific.
"""

import argparse
import json
import statistics
import time

from crypto_microstructure_monitor import fetch
from crypto_options_skew_monitor import expiry_timestamp, payload_row


def number(value):
    return float(value) if isinstance(value, (int, float)) else None


def gamma_rows(rows):
    result = []
    for row in rows:
        option = payload_row(row)
        gamma = number(option.get("gamma"))
        open_interest = number(option.get("open_interest"))
        underlying = number(option.get("underlying_price"))
        strike = number(option.get("strike"))
        if (gamma is None or open_interest is None or underlying is None or strike is None
                or gamma < 0 or open_interest <= 0 or underlying <= 0 or strike <= 0):
            continue
        result.append({
            "strike": strike,
            "option_type": str(option.get("option_type", "unknown")).lower(),
            "gamma": gamma,
            "open_interest": open_interest,
            "underlying_price": underlying,
            "gamma_mass": abs(gamma * open_interest * underlying * underlying),
            "expiry_time": option.get("expiry_time"),
        })
    return result


def summarize_expiry(rows, expiry, now_ts, atm_band):
    points = gamma_rows(rows)
    if not points:
        return {
            "expiry_time": expiry,
            "days_to_expiry": ((expiry_timestamp(expiry) - now_ts) / 86_400.0)
            if expiry_timestamp(expiry) else None,
            "contracts_with_gamma": 0,
            "underlying_price": None,
            "total_gamma_mass": None,
            "near_spot_gamma_mass": None,
            "near_spot_share": None,
            "concentration_at_strike": None,
            "dominant_strike": None,
            "dominant_strike_distance_pct": None,
            "call_gamma_mass": None,
            "put_gamma_mass": None,
        }
    underlying = statistics.median(point["underlying_price"] for point in points)
    total = sum(point["gamma_mass"] for point in points)
    by_strike = {}
    near_spot = 0.0
    call_mass = 0.0
    put_mass = 0.0
    for point in points:
        by_strike[point["strike"]] = by_strike.get(point["strike"], 0.0) + point["gamma_mass"]
        if abs(point["strike"] / underlying - 1.0) <= atm_band:
            near_spot += point["gamma_mass"]
        if point["option_type"] == "call":
            call_mass += point["gamma_mass"]
        elif point["option_type"] == "put":
            put_mass += point["gamma_mass"]
    dominant_strike, dominant_mass = max(by_strike.items(), key=lambda item: (item[1], -item[0]))
    return {
        "expiry_time": expiry,
        "days_to_expiry": ((expiry_timestamp(expiry) - now_ts) / 86_400.0)
        if expiry_timestamp(expiry) else None,
        "contracts_with_gamma": len(points),
        "underlying_price": underlying,
        "total_gamma_mass": total,
        "near_spot_gamma_mass": near_spot,
        "near_spot_share": near_spot / total if total > 0 else None,
        "concentration_at_strike": dominant_mass / total if total > 0 else None,
        "dominant_strike": dominant_strike,
        "dominant_strike_distance_pct": (dominant_strike / underlying - 1.0) * 100.0,
        "call_gamma_mass": call_mass,
        "put_gamma_mass": put_mass,
    }


def classify_gamma(summary, min_near_share, min_concentration):
    near_share = summary.get("near_spot_share")
    concentration = summary.get("concentration_at_strike")
    if near_share is None or concentration is None:
        return "observe_only_missing_gamma"
    if near_share >= min_near_share and concentration >= min_concentration:
        return "near_spot_gamma_concentration"
    if near_share >= min_near_share:
        return "near_spot_gamma_zone"
    return "diffuse_gamma_map"


def enrich_deribit_rows(base_url, rows, max_book_fetches, timeout):
    """Fill missing greeks from the bounded existing Deribit book endpoint."""
    candidates = []
    for row in rows:
        option = payload_row(row)
        if option.get("gamma") is None and option.get("instrument_name"):
            candidates.append(option)
    candidates.sort(key=lambda option: abs(
        (number(option.get("strike")) or 0.0) - (number(option.get("underlying_price")) or 0.0)
    ))
    enriched = []
    errors = []
    fetches = 0
    selected = {id(option): option for option in candidates[:max_book_fetches]}
    for row in rows:
        option = dict(payload_row(row))
        if id(payload_row(row)) in selected:
            fetches += 1
            try:
                book_payload = fetch(base_url, "/options/deribit/book", {
                    "instrument_name": option["instrument_name"],
                    "depth": 1,
                }, timeout)
                book = book_payload.get("book") or {}
                for key in ("gamma", "delta", "theta", "vega", "underlying_price", "open_interest"):
                    if option.get(key) is None and book.get(key) is not None:
                        option[key] = book[key]
                if book_payload.get("error"):
                    errors.append({"instrument_name": option["instrument_name"],
                                   "error": book_payload["error"]})
            except Exception as error:  # keep partial provider coverage visible
                errors.append({"instrument_name": option["instrument_name"], "error": str(error)})
        enriched.append(option)
    return enriched, {
        "candidates_total": len(candidates),
        "requested": fetches,
        "unfetched": max(0, len(candidates) - fetches),
        "errors": errors,
    }


def observe(base_url, currency, venue, expiry_days, atm_band,
            min_near_share, min_concentration, max_book_fetches, timeout):
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
    expiry_rows = sorted(
        grouped.items(),
        key=lambda item: expiry_timestamp(item[0]) or float("inf"),
    )
    summaries = [summarize_expiry(rows, expiry, now_ts, atm_band)
                 for expiry, rows in expiry_rows]
    summaries.sort(key=lambda row: row.get("days_to_expiry") or float("inf"))
    target = min(summaries, key=lambda row: abs((row.get("days_to_expiry") or 0) - expiry_days)) if summaries else None
    enrichment = {"candidates_total": 0, "requested": 0, "unfetched": 0, "errors": []}
    if target and venue.lower() == "deribit":
        target_index = next(index for index, item in enumerate(summaries)
                            if item.get("expiry_time") == target.get("expiry_time"))
        target_expiry = target["expiry_time"]
        raw_rows = grouped[target_expiry]
        enriched_rows, enrichment = enrich_deribit_rows(
            base_url, raw_rows, max_book_fetches, timeout,
        )
        target = summarize_expiry(enriched_rows, target_expiry, now_ts, atm_band)
        summaries[target_index] = target
    if target:
        target["state"] = classify_gamma(target, min_near_share, min_concentration)
    return {
        "currency": currency.upper(),
        "venue": venue,
        "target_expiry": target,
        "available_expiries": len(summaries),
        "expiries": summaries,
        "book_enrichment": enrichment,
        "upstream_errors": payload.get("errors", []) + enrichment["errors"],
        "evidence": [
            "option_chain_available" if summaries else "missing_option_chain",
            "gamma_rows_available" if target and target.get("contracts_with_gamma", 0) else "missing_gamma_or_open_interest",
            "bounded_gamma_book_enrichment",
            "dealer_gamma_sign_not_inferred",
        ],
        "limitations": [
            "open interest does not identify dealer long/short gamma or participant ownership",
            "gamma mass is a relative snapshot proxy, not USD exposure or a hedge ratio",
            "contract multipliers, settlement conventions and stale rows are venue-specific",
            "no historical surface, realized-volatility response, fills, fees or hedge PnL",
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
    parser.add_argument("--min-near-share", type=float, default=0.50)
    parser.add_argument("--min-concentration", type=float, default=0.10)
    parser.add_argument("--max-book-fetches", type=int, default=24,
                        help="bounded Deribit book calls used to fill missing greeks")
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.expiry_days <= 0 or not 0 < options.atm_band < 0.25
            or not 0 <= options.min_near_share <= 1
            or not 0 <= options.min_concentration <= 1
            or options.max_book_fetches < 0
            or options.iterations <= 0 or options.interval_secs < 0):
        parser.error("invalid expiry, band, thresholds, iterations or interval")
    for iteration in range(options.iterations):
        result = observe(options.base_url, options.currency, options.venue,
                         options.expiry_days, options.atm_band,
                         options.min_near_share, options.min_concentration,
                         options.max_book_fetches,
                         options.timeout)
        print(json.dumps({"strategy": "crypto_options_gamma_monitor",
                          "iteration": iteration + 1, **result},
                         ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
