#!/usr/bin/env python3
"""Observe Hyperliquid's predicted funding estimates by venue.

The falsifiable case is intentionally narrow: when the provider publishes a
large difference between named venue estimates for the same coin, is that
cross-venue dispersion persistent in subsequent snapshots?  This monitor only
reports the current provider snapshot.  It does not treat a predicted rate as
settled funding, normalize unknown venue schedules, or place a hedge/order.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/market/predicted-funding?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def dispersion_rows(rows, threshold_bps):
    grouped = {}
    for row in rows:
        symbol = str(row.get("symbol", "")).upper()
        venue = row.get("venue")
        rate = number(row.get("funding_rate"))
        if symbol and isinstance(venue, str) and rate is not None:
            grouped.setdefault(symbol, []).append((venue, rate))
    observations = []
    for symbol, values in sorted(grouped.items()):
        if len(values) < 2:
            state = "observe_only_single_venue_estimate"
            low = high = None
        else:
            low = min(values, key=lambda item: (item[1], item[0]))
            high = max(values, key=lambda item: (item[1], item[0]))
            state = (
                "wide_predicted_funding_gap"
                if (high[1] - low[1]) * 10_000 >= threshold_bps
                else "ordinary_predicted_funding_gap"
            )
        observations.append({
            "symbol": symbol,
            "venue_count": len(values),
            "estimates": {venue: rate * 100.0 for venue, rate in sorted(values)},
            "lowest_venue": low[0] if low else None,
            "highest_venue": high[0] if high else None,
            "dispersion_bps": (high[1] - low[1]) * 10_000 if low and high else None,
            "state": state,
        })
    return observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="hyperliquid")
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--venues", default=None)
    parser.add_argument("--threshold-bps", type=float, default=5.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if options.threshold_bps < 0 or options.iterations <= 0 or options.interval_secs < 0 or options.timeout <= 0:
        parser.error("threshold, iterations, interval and timeout must be valid")
    for iteration in range(options.iterations):
        payload = fetch(options.base_url, {
            "exchange": options.exchange,
            "symbols": options.symbols,
            "venues": options.venues,
        }, options.timeout)
        rows = dispersion_rows(payload.get("funding", []), options.threshold_bps)
        print(json.dumps({
            "strategy": "crypto_predicted_funding_monitor",
            "iteration": iteration + 1,
            "exchange": options.exchange,
            "observations": rows,
            "source_count": len(payload.get("funding", [])),
            "errors": payload.get("errors", []),
            "limitations": [
                "predicted funding is a provider estimate, not settled funding history",
                "venue labels, next funding timestamps and schedules remain provider-specific",
                "dispersion excludes fees, basis, transfer, margin, inventory and execution",
            ],
            "execution": "research_only_no_orders",
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
