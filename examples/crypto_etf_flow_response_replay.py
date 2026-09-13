#!/usr/bin/env python3
"""Replay BTC response after daily spot-ETF flow observations.

The ETF flow file is an explicit external research input; MarketBridge remains
the canonical price interface.  The replay compares large inflow/outflow days
with ordinary flow days at a fixed forward daily-candle horizon.  It does not
claim that ETF flow is causal, complete, or executable.
"""

import argparse
import csv
import json
import statistics
import time
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def parse_flow(value):
    if value is None:
        return None
    text = str(value).strip().replace("$", "").replace(",", "")
    if not text or text in {"-", "—", "N/A", "n/a"}:
        return None
    negative = text.startswith("(") and text.endswith(")")
    text = text.strip("()")
    try:
        result = float(text)
    except ValueError:
        return None
    return -result if negative else result


def _header_key(value):
    return "".join(character for character in str(value).lower() if character.isalnum())


def load_flows(path):
    """Load date + aggregate flow from a Farside-style CSV in USD millions."""
    rows, invalid_rows = [], 0
    with path.open(newline="", encoding="utf-8-sig") as handle:
        reader = csv.DictReader(handle)
        headers = {_header_key(item): item for item in (reader.fieldnames or [])}
        date_key = next((headers[key] for key in ("date", "day", "timestamp") if key in headers), None)
        flow_key = next((headers[key] for key in
                         ("total", "totalflow", "netflow", "netflowmusd", "totalusdm")
                         if key in headers), None)
        if date_key is None or flow_key is None:
            return [], 1
        for row in reader:
            date_text = str(row.get(date_key, "")).strip()
            flow = parse_flow(row.get(flow_key))
            try:
                parsed = datetime.strptime(date_text[:10], "%Y-%m-%d").date()
            except ValueError:
                try:
                    parsed = datetime.strptime(date_text[:10], "%d %b %Y").date()
                except ValueError:
                    invalid_rows += 1
                    continue
            if flow is None:
                invalid_rows += 1
                continue
            rows.append({"date": parsed.isoformat(), "flow_musd": flow})
    deduped = {row["date"]: row for row in rows}
    return [deduped[key] for key in sorted(deduped)], invalid_rows


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            date = datetime.fromtimestamp(timestamp / 1000.0, timezone.utc).date().isoformat()
            points.append((date, close))
    return sorted(dict(points).items())


def classify(flow_musd, threshold_musd):
    if flow_musd >= threshold_musd:
        return "large_inflow"
    if flow_musd <= -threshold_musd:
        return "large_outflow"
    return "ordinary_flow"


def aligned_observations(flows, prices, threshold_musd, horizon_days):
    price_by_date = dict(prices)
    ordered_dates = [date for date, _ in prices]
    observations = []
    for flow in flows:
        if flow["date"] not in price_by_date:
            continue
        try:
            index = ordered_dates.index(flow["date"])
        except ValueError:
            continue
        future_index = index + horizon_days
        if future_index >= len(ordered_dates):
            continue
        baseline, future = price_by_date[flow["date"]], price_by_date[ordered_dates[future_index]]
        observations.append({
            "date": flow["date"],
            "flow_musd": flow["flow_musd"],
            "state": classify(flow["flow_musd"], threshold_musd),
            "forward_date": ordered_dates[future_index],
            "forward_return_pct": (future / baseline - 1.0) * 100.0,
        })
    return observations


def bucket_stats(rows, paper_cost_bps):
    returns = [row["forward_return_pct"] for row in rows]
    adjusted = [value - paper_cost_bps / 100.0 for value in returns]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns) if returns else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)) if returns else None,
        "mean_cost_adjusted_forward_return_pct": statistics.mean(adjusted) if adjusted else None,
        "paper_cost_bps": paper_cost_bps,
    }


def summarize(observations, min_observations, paper_cost_bps):
    by_state = {state: [row for row in observations if row["state"] == state]
                for state in ("large_inflow", "large_outflow", "ordinary_flow")}
    return {
        "by_state": {state: bucket_stats(rows, paper_cost_bps) for state, rows in by_state.items()},
        "verdict": "etf_flow_response_reported"
        if len(observations) >= min_observations else "observe_only_insufficient_aligned_flow_days",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True,
                        help="CSV with date and aggregate ETF flow in USD millions")
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--days", type=float, default=730.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--threshold-musd", type=float, default=100.0)
    parser.add_argument("--horizon-days", type=int, default=1)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.threshold_musd < 0
            or args.horizon_days <= 0 or args.paper_cost_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid days, limits, threshold, horizon, cost or observation arguments")
    flows, invalid_flow_rows = load_flows(args.input)
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    prices = candle_points(payload)
    observations = aligned_observations(flows, prices, args.threshold_musd, args.horizon_days)
    coverage = payload.get("coverage_detail")
    print(json.dumps({
        "strategy": "crypto_etf_flow_response_replay",
        "input": str(args.input),
        "external_flow_source": "caller_supplied_farside_style_csv_usd_millions",
        "market": {"exchange": args.exchange, "market": args.market, "symbol": args.symbol,
                   "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"threshold_musd": args.threshold_musd, "horizon_days": args.horizon_days,
                    "paper_cost_bps": args.paper_cost_bps, "min_observations": args.min_observations},
        "source_counts": {"flow_rows": len(flows), "invalid_flow_rows": invalid_flow_rows,
                           "price_bars": len(prices), "aligned_observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations, args.paper_cost_bps),
        "coverage": coverage,
        "evidence": ["external_flow_manifest_loaded" if flows else "missing_external_flow_manifest",
                      "marketbridge_price_history_available" if prices else "missing_marketbridge_price_history"],
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "ETF flow is a caller-supplied external CSV and is not yet a MarketBridge native historical endpoint",
            "Farside-style daily flow dates and MarketBridge candle dates are aligned by UTC calendar day",
            "ETF settlement/NAV timing, revisions, weekend gaps, causality, fees and execution are not modeled",
            "fixed close-to-close response is descriptive and not an ETF or crypto trading instruction",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
