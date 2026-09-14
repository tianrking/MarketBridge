#!/usr/bin/env python3
"""Replay BTC response after ETF-flow and stablecoin-supply impulse states.

The case tests a narrow, falsifiable version of a common liquidity narrative:
when a trailing ETF-flow window and stablecoin circulating supply move in the
same direction, is the next fixed BTC candle window different from mixed or
neutral liquidity states?  ETF rows are supplied by a caller-owned Farside
CSV/JSONL archive; MarketBridge remains the source of stablecoin history and
price candles.  The result is a descriptive response table, not a causal
liquidity model or a trading instruction.
"""

import argparse
import json
import statistics
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_etf_flow_response_replay import load_flows


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def utc_date(ts_ms):
    return datetime.fromtimestamp(ts_ms / 1000.0, timezone.utc).date().isoformat()


def stablecoin_points(payload):
    """Return one positive circulating-supply point per UTC date."""
    points = []
    for row in payload.get("rows", []):
        ts_ms = row.get("ts_ms")
        value = number(row.get("total_circulating_usd"))
        if isinstance(ts_ms, int) and value is not None and value > 0:
            points.append((utc_date(ts_ms), value, ts_ms))
    return sorted({date: (value, ts_ms) for date, value, ts_ms in points}.items())


def candle_points(payload):
    """Return one positive close per UTC date from MarketBridge candles."""
    points = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            points.append((utc_date(ts_ms), close, ts_ms))
    return sorted({date: (close, ts_ms) for date, close, ts_ms in points}.items())


def _state(sign):
    return "positive" if sign > 0 else "negative" if sign < 0 else "neutral"


def classify_impulse(etf_flow_musd, supply_change_pct, etf_threshold_musd, supply_threshold_pct):
    """Classify two independent impulses without assigning causality."""
    etf_sign = (1 if etf_flow_musd >= etf_threshold_musd else -1
                if etf_flow_musd <= -etf_threshold_musd else 0)
    supply_sign = (1 if supply_change_pct >= supply_threshold_pct else -1
                   if supply_change_pct <= -supply_threshold_pct else 0)
    return f"etf_{_state(etf_sign)}_stablecoin_{_state(supply_sign)}"


def _trailing_flow(flows, date, observations):
    eligible = [row for row in flows if row["date"] <= date]
    if len(eligible) < observations:
        return None
    rows = eligible[-observations:]
    return sum(row["flow_musd"] for row in rows), rows


def _prior_supply(stablecoins, date, change_window_days):
    eligible = [row for row in stablecoins if row[0] <= date]
    if not eligible:
        return None
    current_date, current = eligible[-1][0], eligible[-1][1][0]
    if current_date != date:
        return None
    target = datetime.strptime(current_date, "%Y-%m-%d").date() - timedelta(days=change_window_days)
    prior = [row for row in eligible if datetime.strptime(row[0], "%Y-%m-%d").date() <= target]
    if not prior:
        return None
    previous = prior[-1][1][0]
    return (current, previous, current_date, prior[-1][0])


def aligned_observations(
    flows,
    stablecoins,
    prices,
    flow_window_observations,
    supply_change_window_days,
    etf_threshold_musd,
    supply_threshold_pct,
    horizon_days,
):
    """Join only as-of inputs and measure a later fixed candle response."""
    if flow_window_observations <= 0 or supply_change_window_days <= 0 or horizon_days <= 0:
        raise ValueError("all windows must be positive")
    price_by_date = {date: value[0] for date, value in prices}
    price_dates = [date for date, _ in prices]
    price_index = {date: index for index, date in enumerate(price_dates)}
    rows = []
    for date in price_dates:
        flow = _trailing_flow(flows, date, flow_window_observations)
        supply = _prior_supply(stablecoins, date, supply_change_window_days)
        if flow is None or supply is None:
            continue
        future_index = price_index[date] + horizon_days
        if future_index >= len(price_dates):
            continue
        rolling_flow_musd, flow_rows = flow
        current_supply, previous_supply, supply_date, previous_supply_date = supply
        if previous_supply <= 0:
            continue
        change_pct = (current_supply / previous_supply - 1.0) * 100.0
        future_date = price_dates[future_index]
        current_price, future_price = price_by_date[date], price_by_date[future_date]
        forward = (future_price / current_price - 1.0) * 100.0
        rows.append({
            "date": date,
            "etf_flow_musd": rolling_flow_musd,
            "etf_flow_window_observations": flow_window_observations,
            "etf_flow_window_start": flow_rows[0]["date"],
            "etf_flow_window_end": flow_rows[-1]["date"],
            "stablecoin_supply_date": supply_date,
            "stablecoin_previous_date": previous_supply_date,
            "stablecoin_supply_change_pct": change_pct,
            "state": classify_impulse(
                rolling_flow_musd, change_pct, etf_threshold_musd, supply_threshold_pct,
            ),
            "forward_date": future_date,
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
        })
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize(observations, min_observations):
    states = (
        "etf_positive_stablecoin_positive", "etf_negative_stablecoin_negative",
        "etf_positive_stablecoin_negative", "etf_negative_stablecoin_positive",
        "etf_positive_stablecoin_neutral", "etf_negative_stablecoin_neutral",
        "etf_neutral_stablecoin_positive", "etf_neutral_stablecoin_negative",
        "etf_neutral_stablecoin_neutral",
    )
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    qualifying = sum(stats["observations"] for state, stats in by_state.items()
                     if state != "etf_neutral_stablecoin_neutral")
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "non_neutral_windows": qualifying,
        "verdict": ("liquidity_impulse_response_reported"
                     if qualifying >= min_observations
                     else "observe_only_insufficient_liquidity_impulse_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--etf-flow-csv", type=Path, required=True,
                        help="Farside-style CSV or recorder JSONL with aggregate flow in USD millions")
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--chain", default="all")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--days", type=float, default=1825.0)
    parser.add_argument("--stablecoin-limit", type=int, default=5000)
    parser.add_argument("--candle-limit", type=int, default=1500)
    parser.add_argument("--candle-pages", type=int, default=2)
    parser.add_argument("--flow-window-observations", type=int, default=5)
    parser.add_argument("--supply-change-window-days", type=int, default=7)
    parser.add_argument("--etf-threshold-musd", type=float, default=100.0)
    parser.add_argument("--supply-threshold-pct", type=float, default=1.0)
    parser.add_argument("--horizon-days", type=int, default=7)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.stablecoin_limit <= 5000
            or not 2 <= args.candle_limit <= 1500 or not 1 <= args.candle_pages <= 48
            or args.flow_window_observations <= 0
            or args.supply_change_window_days <= 0 or args.etf_threshold_musd < 0
            or args.supply_threshold_pct < 0 or args.horizon_days <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid days, limits, windows, thresholds or observation arguments")
    flows, invalid_flow_rows = load_flows(args.etf_flow_csv)
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    stablecoin_payload = fetch(args.base_url, "/v1/history/stablecoins", {
        "chain": args.chain, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.stablecoin_limit,
    }, args.timeout)
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.candle_limit, "pages": args.candle_pages,
    }, args.timeout)
    stablecoins = stablecoin_points(stablecoin_payload)
    prices = candle_points(candle_payload)
    observations = aligned_observations(
        flows, stablecoins, prices, args.flow_window_observations,
        args.supply_change_window_days, args.etf_threshold_musd,
        args.supply_threshold_pct, args.horizon_days,
    )
    print(json.dumps({
        "strategy": "crypto_liquidity_impulse_replay",
        "input": str(args.etf_flow_csv),
        "external_flow_source": ("marketbridge_farside_etf_jsonl"
                                  if args.etf_flow_csv.suffix.lower() == ".jsonl"
                                  else "caller_supplied_farside_style_csv_usd_millions"),
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "stablecoin_scope": {"chain": args.chain, "peg": "peggedUSD"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {
            "flow_window_observations": args.flow_window_observations,
            "candle_pages": args.candle_pages,
            "supply_change_window_days": args.supply_change_window_days,
            "etf_threshold_musd": args.etf_threshold_musd,
            "supply_threshold_pct": args.supply_threshold_pct,
            "horizon_days": args.horizon_days,
            "min_observations": args.min_observations,
        },
        "source_counts": {
            "flow_rows": len(flows), "invalid_flow_rows": invalid_flow_rows,
            "stablecoin_rows": len(stablecoins), "price_bars": len(prices),
            "aligned_observations": len(observations),
        },
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": {"stablecoin": stablecoin_payload.get("coverage_detail"),
                     "price": candle_payload.get("coverage_detail")},
        "evidence": ["external_flow_manifest_loaded" if flows else "missing_external_flow_manifest",
                     "stablecoin_history_available" if stablecoins else "missing_stablecoin_history",
                     "marketbridge_price_history_available" if prices else "missing_marketbridge_price_history"],
        "upstream_errors": ([stablecoin_payload["error"]] if stablecoin_payload.get("error") else [])
        + ([candle_payload["error"]] if candle_payload.get("error") else []),
        "limitations": [
            "ETF flow is a caller-supplied daily external series; publication and NAV timing are not modeled",
            "stablecoin circulating supply is not exchange inventory, bridge flow or confirmed buying power",
            "the two inputs have different clocks and missing dates; incomplete as-of windows are skipped",
            "UTC-date alignment, fixed close-to-close response, revisions, causality, fees and execution are not modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
