#!/usr/bin/env python3
"""Replay a public Polymarket price-shock continuation hypothesis.

Some public X narratives describe a short window in which new evidence moves
the underlying probability before the market fully reprices.  This example
does not claim to observe the evidence or to trade it.  It turns that narrative
into a falsifiable, read-only test: after a sharp one-step probability move in
the public price history, does the same-direction move persist for the next N
history points?

The result is a descriptive replay.  It excludes fill quality, fees, latency,
resolution rules and the causal source of each price move; missing context is
reported instead of being treated as a signal.
"""

import argparse
import json
import sys
from statistics import mean, median
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def numeric_points(payload):
    """Return sorted ``(timestamp_seconds, probability)`` points."""

    points = []
    for row in payload.get("history", []):
        timestamp = row.get("t")
        price = row.get("p")
        if isinstance(timestamp, (int, float)) and isinstance(price, (int, float)):
            if price == price and 0.0 <= price <= 1.0:
                points.append((float(timestamp), float(price)))
    return sorted(set(points))


def detect_shocks(points, shock_bps, min_price, max_price, cooldown_points):
    """Find non-overlapping adjacent changes large enough to investigate."""

    shocks = []
    next_allowed = 0
    for index in range(1, len(points)):
        if index < next_allowed:
            continue
        before_ts, before_price = points[index - 1]
        after_ts, after_price = points[index]
        delta_bps = (after_price - before_price) * 10_000.0
        if (
            abs(delta_bps) < shock_bps
            or not min_price <= after_price <= max_price
            or after_ts <= before_ts
        ):
            continue
        shocks.append(
            {
                "index": index,
                "before_ts": before_ts,
                "ts": after_ts,
                "before_price": before_price,
                "price": after_price,
                "shock_bps": delta_bps,
                "direction": "up" if delta_bps > 0 else "down",
            }
        )
        next_allowed = index + max(1, cooldown_points)
    return shocks


def score_shocks(points, shocks, horizon_points):
    """Attach forward move and continuation labels without extrapolation."""

    scored = []
    for shock in shocks:
        exit_index = shock["index"] + horizon_points
        if exit_index >= len(points):
            continue
        exit_ts, exit_price = points[exit_index]
        forward_bps = (exit_price - shock["price"]) * 10_000.0
        shock_bps = shock["shock_bps"]
        scored.append(
            {
                **shock,
                "exit_ts": exit_ts,
                "exit_price": exit_price,
                "forward_bps": forward_bps,
                "same_direction": forward_bps * shock_bps > 0.0,
                "reversal": forward_bps * shock_bps < 0.0,
            }
        )
    return scored


def select_market(markets, market_id, market_query, outcome):
    query = market_query.casefold() if market_query else None
    target = market_id.casefold() if market_id else None
    for market in markets:
        if market.get("status") != "active":
            continue
        if target and str(market.get("market_id", "")).casefold() != target:
            continue
        if query and query not in str(market.get("question", "")).casefold():
            continue
        outcomes = market.get("outcomes", [])
        token_ids = market.get("clob_token_ids", [])
        for label, token_id in zip(outcomes, token_ids):
            if str(label).casefold() == outcome.casefold():
                return market, token_id
    return None, None


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    selector = parser.add_mutually_exclusive_group(required=True)
    selector.add_argument("--market-id", help="Polymarket condition/market id")
    selector.add_argument("--market-query", help="case-insensitive text in the market question")
    parser.add_argument("--outcome", default="Yes", help="outcome label whose probability history is replayed")
    parser.add_argument("--market-limit", type=int, default=500)
    parser.add_argument("--max-offset", type=int, default=500)
    parser.add_argument("--interval", choices=("1m", "1h", "1d", "6h", "1w", "max", "all"), default="1m")
    parser.add_argument("--fidelity", type=int, default=10)
    parser.add_argument("--shock-bps", type=float, default=100.0)
    parser.add_argument("--horizon-points", type=int, default=3)
    parser.add_argument("--cooldown-points", type=int, default=1)
    parser.add_argument("--min-price", type=float, default=0.05)
    parser.add_argument("--max-price", type=float, default=0.95)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--max-shocks", type=int, default=100)
    parser.add_argument("--timeout", type=float, default=30.0)
    return parser.parse_args()


def main():
    options = parse_args()
    if options.market_limit <= 0 or options.max_offset < 0:
        raise SystemExit("market-limit must be positive and max-offset cannot be negative")
    if options.fidelity <= 0 or options.shock_bps <= 0 or options.horizon_points <= 0:
        raise SystemExit("fidelity, shock-bps and horizon-points must be positive")
    if options.interval == "1m" and options.fidelity < 10:
        raise SystemExit("Polymarket requires fidelity >= 10 minutes for a 1m history interval")
    if options.cooldown_points <= 0 or options.max_shocks <= 0 or options.min_observations <= 0:
        raise SystemExit("cooldown-points, max-shocks and min-observations must be positive")
    if not 0.0 <= options.min_price <= options.max_price <= 1.0:
        raise SystemExit("price bounds must satisfy 0 <= min-price <= max-price <= 1")

    markets_payload = fetch(
        options.base_url,
        "/polymarket/markets",
        {
            "limit": min(options.market_limit, 500),
            "max_offset": min(options.max_offset, 5000),
        },
        options.timeout,
    )
    market, token_id = select_market(
        markets_payload.get("markets", []),
        options.market_id,
        options.market_query,
        options.outcome,
    )
    if not market or not token_id:
        raise SystemExit("no active market/outcome matched the selector")

    history_payload = fetch(
        options.base_url,
        "/polymarket/prices-history",
        {
            "token_id": token_id,
            "interval": options.interval,
            "fidelity": options.fidelity,
        },
        options.timeout,
    )
    points = numeric_points(history_payload)
    shocks = detect_shocks(
        points,
        options.shock_bps,
        options.min_price,
        options.max_price,
        options.cooldown_points,
    )[: options.max_shocks]
    observations = score_shocks(points, shocks, options.horizon_points)
    continuation = [row["same_direction"] for row in observations]
    forward = [row["forward_bps"] for row in observations]
    enough = len(observations) >= options.min_observations
    continuation_fraction = mean(continuation) if continuation else None
    verdict = (
        "descriptive continuation evidence"
        if enough and continuation_fraction > 0.5
        else "observe only"
    )
    print(
        json.dumps(
            {
                "strategy": "polymarket_price_shock_replay",
                "hypothesis": "a sharp public probability update may persist for the next history window",
                "market": {
                    "market_id": market.get("market_id"),
                    "question": market.get("question"),
                    "outcome": options.outcome,
                    "token_id": token_id,
                    "expiry_time": market.get("expiry_time"),
                },
                "filters": {
                    "interval": options.interval,
                    "fidelity": options.fidelity,
                    "shock_bps": options.shock_bps,
                    "horizon_points": options.horizon_points,
                    "cooldown_points": options.cooldown_points,
                    "min_price": options.min_price,
                    "max_price": options.max_price,
                },
                "history_points": len(points),
                "shocks_detected": len(shocks),
                "observations": observations,
                "summary": {
                    "scored_observations": len(observations),
                    "enough_observations": enough,
                    "continuation_fraction": continuation_fraction,
                    "mean_forward_bps": mean(forward) if forward else None,
                    "median_forward_bps": median(forward) if forward else None,
                    "reversal_fraction": (
                        mean(row["reversal"] for row in observations) if observations else None
                    ),
                },
                "verdict": verdict,
                "missing_context": [
                    "the causal news/evidence timestamp is not in the public price history",
                    "public history does not model queue position, fills, fees, latency or resolution",
                    "one market and one outcome are not a cross-market validation sample",
                ],
                "execution": "research_only_no_orders",
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        print(f"MarketBridge request or response failed: {error}", file=sys.stderr)
        raise SystemExit(1)
