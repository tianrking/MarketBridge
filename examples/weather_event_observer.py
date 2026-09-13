#!/usr/bin/env python3
"""Inspect an Open-Meteo daily maximum-temperature event through MarketBridge.

This turns a weather-market narrative into an explicit observation: whether the
provider's daily maximum is inside a requested bucket. It does not convert a
deterministic forecast into a probability and does not place trades.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode(params)
    request = Request(f"{base_url.rstrip('/')}/v1/external/weather?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--latitude", type=float, required=True)
    parser.add_argument("--longitude", type=float, required=True)
    parser.add_argument("--date", required=True, help="YYYY-MM-DD")
    parser.add_argument("--min-temp", type=float, required=True)
    parser.add_argument("--max-temp", type=float, required=True)
    parser.add_argument("--mode", choices=("forecast", "archive"), default="forecast")
    parser.add_argument("--forecast-days", type=int, default=7)
    parser.add_argument("--timezone", default="UTC")
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if options.min_temp > options.max_temp:
        parser.error("--min-temp must be <= --max-temp")

    params = {
        "latitude": options.latitude,
        "longitude": options.longitude,
        "mode": options.mode,
        "timezone": options.timezone,
        "daily": "temperature_2m_max,temperature_2m_min,precipitation_sum,weather_code",
    }
    if options.mode == "forecast":
        params["forecast_days"] = options.forecast_days
    else:
        params["start_date"] = options.date
        params["end_date"] = options.date

    payload = fetch(options.base_url, params, options.timeout)
    daily = payload.get("data", {}).get("daily", {})
    dates = daily.get("time", [])
    maxima = daily.get("temperature_2m_max", [])
    observations = []
    for date, maximum in zip(dates, maxima):
        if date != options.date or not isinstance(maximum, (int, float)):
            continue
        observations.append({
            "date": date,
            "temperature_2m_max": maximum,
            "in_bucket": options.min_temp <= maximum <= options.max_temp,
        })
    print(json.dumps({
        "source": payload.get("source"),
        "mode": options.mode,
        "latitude": options.latitude,
        "longitude": options.longitude,
        "bucket": {"min_temp": options.min_temp, "max_temp": options.max_temp},
        "observations": observations,
        "limitations": [
            "one provider/model is not a calibrated probability distribution",
            "market location, bucket boundaries and resolution rules require independent identity evidence",
        ],
    }, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
