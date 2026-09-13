#!/usr/bin/env python3
"""Observe aggregate derivatives positioning context from MarketBridge.

CoinGlass is an optional keyed aggregate source. This monitor combines only
the latest available funding, long/short ratio, OI, basis and liquidation
metrics; it does not treat aggregate ratios as trader ownership or a trade
signal.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/external/signals?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def summarize_signals(signals, long_short_high, long_short_low, liquidation_threshold):
    metrics = {}
    for row in signals:
        metric = str(row.get("metric", "")).lower()
        value = number(row.get("value"))
        if metric and value is not None:
            metrics[metric] = value
    funding = metrics.get("funding_rate")
    ratio = metrics.get("long_short_ratio")
    liquidation = metrics.get("liquidation")
    if ratio is None or funding is None:
        state = "observe_only_missing_positioning_metrics"
    elif ratio >= long_short_high and funding > 0:
        state = "long_crowding_context"
    elif ratio <= long_short_low and funding < 0:
        state = "short_crowding_context"
    else:
        state = "mixed_or_neutral_positioning_context"
    return {
        "metrics": metrics,
        "state": state,
        "liquidation_activity": liquidation is not None and liquidation >= liquidation_threshold,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTC")
    parser.add_argument("--long-short-high", type=float, default=1.2)
    parser.add_argument("--long-short-low", type=float, default=0.8)
    parser.add_argument("--liquidation-threshold", type=float, default=1_000_000.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.long_short_low < 0 or args.long_short_high <= args.long_short_low
            or args.liquidation_threshold < 0 or args.timeout <= 0):
        parser.error("invalid positioning or timeout thresholds")
    payload = fetch(args.base_url, {
        "sources": "coinglass", "symbols": args.symbol,
        "metrics": "funding_rate,long_short_ratio,open_interest,basis,liquidation,options_open_interest",
    }, args.timeout)
    signals = payload.get("signals", [])
    result = summarize_signals(signals, args.long_short_high, args.long_short_low,
                               args.liquidation_threshold)
    print(json.dumps({
        "strategy": "crypto_derivatives_sentiment_monitor",
        "symbol": args.symbol.upper(),
        "filters": {"long_short_high": args.long_short_high,
                    "long_short_low": args.long_short_low,
                    "liquidation_threshold": args.liquidation_threshold},
        "summary": result,
        "signal_rows": signals,
        "evidence": [
            "coinglass_signal_rows_available" if signals else "missing_or_unconfigured_coinglass_signals",
            "positioning_metrics_available" if result["state"] != "observe_only_missing_positioning_metrics"
            else "missing_positioning_metrics",
        ],
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "CoinGlass is optional keyed aggregate data with provider-specific retention and semantics",
            "long/short ratios and OI are aggregate context, not trader-side ownership",
            "a current snapshot is not a historical replay or causal forecast",
            "no order, wallet, allocation or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
