#!/usr/bin/env python3
"""Compare a weather bucket observation with a Polymarket YES ask.

This is a research-only implementation of the public "pressure differential"
narrative: update an external weather observation first, then inspect whether a
matching market is still priced below a user-supplied investigation threshold.
The weather provider is not a calibrated probability distribution, so the
script reports an observation and a candidate for review rather than an edge
or an order instruction.
"""

import argparse
import json
import sys
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def market_tokens(market):
    outcomes = market.get("outcomes", [])
    token_ids = market.get("clob_token_ids", [])
    if not isinstance(outcomes, list) or not isinstance(token_ids, list):
        return None, None
    mapping = {
        str(outcome).strip().lower(): token_id
        for outcome, token_id in zip(outcomes, token_ids)
    }
    return mapping.get("yes"), mapping.get("no")


def args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--market-query", required=True, help="case-insensitive text in the market question")
    parser.add_argument("--latitude", type=float, required=True)
    parser.add_argument("--longitude", type=float, required=True)
    parser.add_argument("--date", required=True, help="YYYY-MM-DD")
    parser.add_argument("--min-temp", type=float, required=True)
    parser.add_argument("--max-temp", type=float, required=True)
    parser.add_argument("--max-yes-ask", type=float, default=0.25)
    parser.add_argument("--mode", choices=("forecast", "archive"), default="forecast")
    parser.add_argument("--market-limit", type=int, default=100)
    parser.add_argument("--timeout", type=float, default=30.0)
    return parser.parse_args()


def main():
    options = args()
    if options.min_temp > options.max_temp:
        raise SystemExit("--min-temp must be <= --max-temp")
    if not 0 <= options.max_yes_ask <= 1:
        raise SystemExit("--max-yes-ask must be between 0 and 1")
    if options.market_limit <= 0:
        raise SystemExit("--market-limit must be positive")

    weather_params = {
        "latitude": options.latitude,
        "longitude": options.longitude,
        "mode": options.mode,
        "timezone": "UTC",
        "daily": "temperature_2m_max,temperature_2m_min,weather_code",
    }
    if options.mode == "forecast":
        weather_params["forecast_days"] = 16
    else:
        weather_params["start_date"] = options.date
        weather_params["end_date"] = options.date
    weather = fetch(options.base_url, "/v1/external/weather", weather_params, options.timeout)
    daily = weather.get("data", {}).get("daily", {})
    observations = {
        date: maximum
        for date, maximum in zip(daily.get("time", []), daily.get("temperature_2m_max", []))
        if isinstance(maximum, (int, float))
    }
    maximum = observations.get(options.date)
    in_bucket = (
        maximum is not None
        and options.min_temp <= maximum <= options.max_temp
    )

    markets_payload = fetch(
        options.base_url,
        "/polymarket/markets",
        {"limit": options.market_limit, "max_offset": options.market_limit},
        options.timeout,
    )
    query = options.market_query.casefold()
    markets = [
        market for market in markets_payload.get("markets", [])
        if market.get("status") == "active"
        and query in str(market.get("question", "")).casefold()
    ]
    token_pairs = []
    for market in markets:
        yes_id, no_id = market_tokens(market)
        if yes_id:
            token_pairs.append((market, yes_id, no_id))
    token_ids = sorted({token_id for _, yes_id, no_id in token_pairs for token_id in (yes_id, no_id) if token_id})
    books_payload = fetch(
        options.base_url,
        "/polymarket/books",
        {"token_ids": ",".join(token_ids)},
        options.timeout,
    ) if token_ids else {"books": []}
    books = {
        book.get("asset_id"): book
        for book in books_payload.get("books", [])
        if book.get("asset_id")
    }

    candidates = []
    for market, yes_id, _ in token_pairs:
        yes_ask = books.get(yes_id, {}).get("best_ask")
        if not isinstance(yes_ask, (int, float)):
            continue
        if in_bucket and yes_ask <= options.max_yes_ask:
            candidates.append({
                "market_id": market.get("market_id"),
                "question": market.get("question"),
                "yes_ask": yes_ask,
                "forecast_max_temperature": maximum,
                "forecast_in_bucket": in_bucket,
            })

    print(json.dumps({
        "strategy": "weather_pressure_differential",
        "market_query": options.market_query,
        "weather": {
            "source": weather.get("source"),
            "date": options.date,
            "max_temperature": maximum,
            "bucket": {"min_temp": options.min_temp, "max_temp": options.max_temp},
            "in_bucket": in_bucket,
        },
        "markets_scanned": len(markets),
        "candidates": candidates,
        "execution": "research_only_no_orders",
        "limitations": [
            "one weather provider/model is not a calibrated probability distribution",
            "market question, location, bucket and resolution rules require independent identity checks",
            "snapshot asks omit queue position, fees, slippage, latency and fill probability",
        ],
    }, ensure_ascii=False, indent=2, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        print(f"MarketBridge request or response failed: {error}", file=sys.stderr)
        raise SystemExit(1)
