#!/usr/bin/env python3
"""Observe a paper bull-call-spread quote from MarketBridge option chains.

This example turns a public options-flow idea into a falsifiable snapshot
hypothesis: for one expiry, a lower-strike call ask and a higher-strike call
bid may form a quoted debit below the strike width.  It only reports the
observable legs and paper payoff geometry; it never submits an order.
"""

import argparse
import json
import statistics
import time

from crypto_microstructure_monitor import fetch
from crypto_options_skew_monitor import expiry_timestamp, payload_row


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def median_or_none(values):
    return statistics.median(values) if values else None


def quote_price(option, side):
    """Return a paper entry price and whether it came from a two-sided quote."""
    direct = number(option.get("ask_price" if side == "long" else "bid_price"))
    if direct is not None and direct >= 0:
        return direct, "quoted"
    mark = number(option.get("mark_price"))
    if mark is not None and mark >= 0:
        return mark, "mark_only"
    return None, "missing"


def select_legs(calls, underlying, long_moneyness, short_moneyness):
    if underlying is None or underlying <= 0 or len(calls) < 2:
        return None
    candidates = []
    for lower in calls:
        lower_strike = number(lower.get("strike"))
        if lower_strike is None or lower_strike <= 0:
            continue
        for upper in calls:
            upper_strike = number(upper.get("strike"))
            if upper_strike is None or upper_strike <= lower_strike:
                continue
            score = abs(lower_strike / underlying - long_moneyness)
            score += abs(upper_strike / underlying - short_moneyness)
            candidates.append((score, lower_strike, upper_strike, lower, upper))
    if not candidates:
        return None
    candidates.sort(key=lambda item: (item[0], item[2] - item[1]))
    _, _, _, lower, upper = candidates[0]
    return lower, upper


def summarize_spread(rows, expiry, now_ts, long_moneyness, short_moneyness):
    options = [payload_row(row) for row in rows]
    calls = [row for row in options if str(row.get("option_type", "")).lower() == "call"]
    underlyings = [number(row.get("underlying_price")) for row in calls]
    underlying = median_or_none([value for value in underlyings if value is not None and value > 0])
    selected = select_legs(calls, underlying, long_moneyness, short_moneyness)
    result = {
        "expiry_time": expiry,
        "days_to_expiry": ((expiry_timestamp(expiry) - now_ts) / 86_400.0)
        if expiry_timestamp(expiry) else None,
        "underlying_price": underlying,
        "call_contracts": len(calls),
        "state": "observe_only_missing_call_pair",
        "legs": None,
        "debit": None,
        "width": None,
        "breakeven": None,
        "max_profit": None,
        "max_profit_to_debit": None,
    }
    if selected is None:
        return result
    lower, upper = selected
    lower_strike = number(lower.get("strike"))
    upper_strike = number(upper.get("strike"))
    long_price, long_source = quote_price(lower, "long")
    short_price, short_source = quote_price(upper, "short")
    width = upper_strike - lower_strike if lower_strike is not None and upper_strike is not None else None
    debit = long_price - short_price if long_price is not None and short_price is not None else None
    quote_quality = "quoted" if long_source == short_source == "quoted" else (
        "mark_only" if long_source != "missing" and short_source != "missing" else "missing")
    result.update({
        "state": "observe_only_missing_call_quotes" if debit is None else (
            "observe_only_debit_not_positive" if debit <= 0 else (
                "observe_only_debit_not_below_width" if width is not None and debit >= width else (
                    "bull_call_spread_mark_only" if quote_quality == "mark_only" else "bull_call_spread_quote_available"))),
        "quote_quality": quote_quality,
        "legs": {
            "long_call": {"strike": lower_strike, "ask_price": number(lower.get("ask_price")),
                           "mark_price": number(lower.get("mark_price")), "entry_price": long_price,
                           "entry_source": long_source, "delta": number(lower.get("delta")),
                           "mark_iv": number(lower.get("mark_iv")), "open_interest": number(lower.get("open_interest"))},
            "short_call": {"strike": upper_strike, "bid_price": number(upper.get("bid_price")),
                            "mark_price": number(upper.get("mark_price")), "entry_price": short_price,
                            "entry_source": short_source, "delta": number(upper.get("delta")),
                            "mark_iv": number(upper.get("mark_iv")), "open_interest": number(upper.get("open_interest"))},
        },
        "debit": debit,
        "width": width,
        "breakeven": lower_strike + debit if lower_strike is not None and debit is not None else None,
        "max_profit": width - debit if width is not None and debit is not None else None,
        "max_profit_to_debit": ((width - debit) / debit)
        if width is not None and debit is not None and debit > 0 else None,
    })
    return result


def observe(base_url, currency, venue, expiry_days, long_moneyness, short_moneyness, timeout):
    payload = fetch(base_url, "/v1/options/chains", {
        "venue": venue, "currency": currency, "option_type": "call", "include_stale": "false",
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
    summaries = [summarize_spread(rows, expiry, now_ts, long_moneyness, short_moneyness)
                 for expiry, rows in grouped.items()]
    summaries.sort(key=lambda row: row.get("days_to_expiry") or float("inf"))
    target = min(summaries, key=lambda row: abs((row.get("days_to_expiry") or 0) - expiry_days)) if summaries else None
    return {
        "currency": currency.upper(),
        "venue": venue,
        "target_expiry": target,
        "available_expiries": len(summaries),
        "stale_rows_included": False,
        "upstream_errors": payload.get("errors", []),
        "evidence": [
            "option_chain_available" if summaries else "missing_option_chain",
            "call_pair_available" if target and target.get("legs") else "missing_call_pair",
            "paper_debit_below_width" if target and target.get("state") in {
                "bull_call_spread_quote_available", "bull_call_spread_mark_only"} else "debit_not_validated",
        ],
        "limitations": [
            "leg selection is a moneyness-nearest heuristic, not a delta-targeted spread selector",
            "mark-only legs are not executable quotes; bid/ask freshness and size are not available here",
            "max profit, breakeven and ratio ignore fees, slippage, settlement, margin, early exercise and hedge costs",
            "this snapshot does not estimate forward returns or option PnL",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--long-moneyness", type=float, default=0.95)
    parser.add_argument("--short-moneyness", type=float, default=1.05)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.expiry_days <= 0 or options.long_moneyness <= 0
            or options.short_moneyness <= options.long_moneyness or options.iterations <= 0
            or options.interval_secs < 0):
        parser.error("invalid expiry, moneyness, iteration or interval arguments")
    for iteration in range(options.iterations):
        result = observe(options.base_url, options.currency, options.venue, options.expiry_days,
                         options.long_moneyness, options.short_moneyness, options.timeout)
        print(json.dumps({"strategy": "crypto_options_bull_call_spread_monitor",
                          "iteration": iteration + 1, **result}, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
