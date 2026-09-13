#!/usr/bin/env python3
"""Calibrate a weather-market manifest against archived MarketBridge weather.

The manifest is intentionally explicit: callers provide the verified market
identity, location, resolution date, temperature bucket and resolved outcome.
MarketBridge supplies the archived weather observation; this script reports
whether the bucket agrees with settlement and, when supplied, descriptive
market-price calibration. It never infers a market question from text and never
places an order.
"""

import argparse
import json
import math
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode(params)
    request = Request(f"{base_url.rstrip('/')}/v1/external/weather?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def yes_outcome(value):
    normalized = str(value or "").strip().lower()
    if normalized in {"yes", "true", "1"}:
        return True
    if normalized in {"no", "false", "0"}:
        return False
    return None


def archived_maximum(payload, date):
    daily = payload.get("data", {}).get("daily", {})
    for observed_date, maximum in zip(daily.get("time", []), daily.get("temperature_2m_max", [])):
        if observed_date == date and isinstance(maximum, (int, float)):
            return float(maximum)
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True, help="JSONL with verified market/event identity fields")
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if options.limit <= 0:
        raise SystemExit("--limit must be positive")

    events = []
    errors = []
    with options.manifest.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            if len(events) >= options.limit:
                break
            try:
                event = json.loads(line)
            except json.JSONDecodeError as error:
                errors.append({"line": line_number, "error": str(error)})
                continue
            required = ("market_id", "latitude", "longitude", "date", "min_temp", "max_temp", "resolved_outcome")
            missing = [field for field in required if field not in event]
            if missing:
                errors.append({"line": line_number, "error": f"missing fields: {','.join(missing)}"})
                continue
            events.append(event)

    observations = []
    for event in events:
        try:
            payload = fetch(options.base_url, {
                "latitude": event["latitude"],
                "longitude": event["longitude"],
                "mode": "archive",
                "timezone": event.get("timezone", "UTC"),
                "start_date": event["date"],
                "end_date": event["date"],
                "daily": "temperature_2m_max,temperature_2m_min,weather_code",
            }, options.timeout)
            maximum = archived_maximum(payload, event["date"])
            resolved_yes = yes_outcome(event["resolved_outcome"])
            in_bucket = (
                maximum is not None
                and float(event["min_temp"]) <= maximum <= float(event["max_temp"])
            )
            row = {
                "market_id": event["market_id"],
                "question": event.get("question"),
                "date": event["date"],
                "temperature_2m_max": maximum,
                "bucket": {"min_temp": event["min_temp"], "max_temp": event["max_temp"]},
                "weather_in_bucket": in_bucket,
                "resolved_yes": resolved_yes,
                "weather_matches_resolution": resolved_yes is not None and in_bucket == resolved_yes,
            }
            yes_price = event.get("yes_price")
            if isinstance(yes_price, (int, float)) and 0.0 <= yes_price <= 1.0 and resolved_yes is not None:
                probability = float(yes_price)
                target = 1.0 if resolved_yes else 0.0
                row["yes_price"] = probability
                row["brier"] = (probability - target) ** 2
                safe_probability = min(max(probability, 1e-9), 1.0 - 1e-9)
                row["log_loss"] = -math.log(safe_probability if target else 1.0 - safe_probability)
            observations.append(row)
        except (OSError, ValueError, KeyError) as error:
            errors.append({"market_id": event.get("market_id"), "error": str(error)})

    settled = [row for row in observations if row["resolved_yes"] is not None]
    priced = [row for row in settled if "brier" in row]
    print(json.dumps({
        "strategy": "weather_market_calibration",
        "manifest": str(options.manifest),
        "events_loaded": len(events),
        "observations": observations,
        "summary": {
            "settled_events": len(settled),
            "weather_resolution_accuracy": (
                sum(row["weather_matches_resolution"] for row in settled) / len(settled)
                if settled else None
            ),
            "priced_events": len(priced),
            "market_brier": sum(row["brier"] for row in priced) / len(priced) if priced else None,
            "market_log_loss": sum(row["log_loss"] for row in priced) / len(priced) if priced else None,
        },
        "errors": errors,
        "limitations": [
            "manifest identity, location and resolution rule must be independently verified",
            "one archived weather provider/model is an observation, not a calibrated probability",
            "market prices are descriptive unless entry timing, fees and fills are separately modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
