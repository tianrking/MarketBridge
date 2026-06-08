#!/usr/bin/env python3
import argparse
import csv
import json
import math
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime
from html import escape
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


BINANCE_FUNDING_URL = "https://fapi.binance.com/fapi/v1/fundingRate"
RETRYABLE_HTTP_CODES = {408, 429, 500, 502, 503, 504}
COLORS = [
    "#2563eb",
    "#dc2626",
    "#059669",
    "#7c3aed",
    "#ea580c",
    "#0891b2",
    "#be123c",
    "#4d7c0f",
]


@dataclass(frozen=True)
class FundingPoint:
    ts_ms: int
    funding_pct: float


@dataclass(frozen=True)
class AnomalyRun:
    symbol: str
    start_ms: int
    end_ms: int
    duration_hours: float
    points: int
    min_funding_pct: float
    max_funding_pct: float


def parse_args():
    parser = argparse.ArgumentParser(
        description=(
            "Fetch and visualize Binance USDT-M perpetual funding-rate curves. "
            "Pass --symbol for one contract or --symbols for one combined chart."
        )
    )
    parser.add_argument("--symbol", help="Contract symbol, for example HOMEUSDT.")
    parser.add_argument(
        "--symbols",
        help="Comma-separated symbols for one combined chart, for example EDENUSDT,HOMEUSDT,GUNUSDT.",
    )
    parser.add_argument("--days", type=float, default=30.0, help="Lookback window in days.")
    parser.add_argument("--quote", default="USDT", help="Default quote to append when a base asset is passed.")
    parser.add_argument(
        "--no-append-quote",
        action="store_true",
        help="Do not append --quote to symbols that do not already end with a known quote.",
    )
    parser.add_argument("--exchange", default="binance", help="MarketBridge exchange name.")
    parser.add_argument("--base-url", default="http://127.0.0.1:8080", help="MarketBridge base URL.")
    parser.add_argument("--source", choices=["marketbridge", "binance"], default="marketbridge")
    parser.add_argument("--no-fallback", action="store_true", help="Do not fall back to direct Binance REST.")
    parser.add_argument("--output-dir", default=".", help="Directory for generated SVG and CSV.")
    parser.add_argument("--no-png", action="store_true", help="Skip PNG rendering and write only SVG + CSV.")
    parser.add_argument("--png-size", type=int, default=3200, help="Long-side size for rendered PNG output.")
    parser.add_argument("--no-report", action="store_true", help="Skip the multi-panel report image.")
    parser.add_argument(
        "--threshold-pct",
        type=float,
        default=-0.2,
        help="Highlight anomaly runs at or below this funding percent.",
    )
    parser.add_argument(
        "--min-run-hours",
        type=float,
        default=0.0,
        help="Only label/write anomaly runs whose duration is at least this many hours.",
    )
    parser.add_argument(
        "--chart-mode",
        choices=["combined", "separate", "both"],
        default="both",
        help="Write one combined chart, one chart per symbol, or both.",
    )
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--retries", type=int, default=3)
    return parser.parse_args()


def svg_size(svg_path):
    text = svg_path.read_text(encoding="utf-8")
    width = re.search(r'<svg[^>]*\bwidth="([0-9.]+)"', text)
    height = re.search(r'<svg[^>]*\bheight="([0-9.]+)"', text)
    if not width or not height:
        raise RuntimeError(f"Cannot read SVG size from {svg_path}")
    return float(width.group(1)), float(height.group(1))


def fetch_json(url, timeout, retries, headers=None):
    request = Request(url, headers=headers or {})
    attempts = max(1, retries)
    last_error = None

    for attempt in range(1, attempts + 1):
        try:
            with urlopen(request, timeout=timeout) as response:
                return json.load(response)
        except HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            last_error = f"HTTP {error.code}: {body}"
            if error.code not in RETRYABLE_HTTP_CODES or attempt == attempts:
                raise RuntimeError(last_error) from error
        except URLError as error:
            last_error = f"Cannot connect: {error}"
            if attempt == attempts:
                raise RuntimeError(last_error) from error
        time.sleep(min(2 ** (attempt - 1), 8))

    raise RuntimeError(last_error or "request failed")


def parse_symbols(args):
    if args.symbol and args.symbols:
        raise SystemExit("Specify either --symbol or --symbols, not both")
    raw = args.symbols if args.symbols else args.symbol
    if not raw:
        raise SystemExit("Specify --symbol HOMEUSDT or --symbols EDENUSDT,HOMEUSDT")
    symbols = []
    seen = set()
    known_quotes = ("USDT", "USDC", "BUSD", "USD", "BTC", "ETH")
    quote = args.quote.strip().upper()
    for item in raw.split(","):
        symbol = item.strip().upper()
        if symbol and not args.no_append_quote and not symbol.endswith(known_quotes):
            symbol = f"{symbol}{quote}"
        if symbol and symbol not in seen:
            symbols.append(symbol)
            seen.add(symbol)
    if not symbols:
        raise SystemExit("No symbols provided")
    return symbols


def safe_stem(value):
    safe = re.sub(r"[^\w._-]+", "_", value)
    safe = safe.strip("._-")
    return safe or "symbol"


def marketbridge_points(args, symbol, start_ms, end_ms):
    params = {
        "exchange": args.exchange,
        "symbol": symbol,
        "candle_type": "funding_rate",
        "start_ms": str(start_ms),
        "end_ms": str(end_ms),
        "limit": "1000",
    }
    url = f"{args.base_url.rstrip('/')}/v1/history/candles?{urlencode(params)}"
    payload = fetch_json(url, timeout=args.timeout, retries=args.retries)
    if payload.get("error"):
        raise RuntimeError(payload["error"])

    candles = payload.get("candles", [])
    if not isinstance(candles, list):
        raise RuntimeError(f"unexpected MarketBridge response: {payload}")

    points = [
        FundingPoint(ts_ms=int(row["open_time_ms"]), funding_pct=float(row["close"]) * 100.0)
        for row in candles
        if isinstance(row, dict) and row.get("open_time_ms") is not None and row.get("close") is not None
    ]
    return sorted(points, key=lambda point: point.ts_ms)


def binance_points(args, symbol, start_ms, end_ms):
    params = {
        "symbol": symbol,
        "startTime": str(start_ms),
        "endTime": str(end_ms),
        "limit": "1000",
    }
    url = f"{BINANCE_FUNDING_URL}?{urlencode(params)}"
    payload = fetch_json(url, timeout=args.timeout, retries=args.retries)
    if not isinstance(payload, list):
        raise RuntimeError(f"unexpected Binance response: {payload}")

    points = [
        FundingPoint(ts_ms=int(row["fundingTime"]), funding_pct=float(row["fundingRate"]) * 100.0)
        for row in payload
        if isinstance(row, dict) and row.get("fundingTime") is not None and row.get("fundingRate") is not None
    ]
    return sorted(points, key=lambda point: point.ts_ms)


def load_symbol_points(args, symbol, start_ms, end_ms):
    if args.source == "binance":
        return binance_points(args, symbol, start_ms, end_ms), "binance_direct"

    try:
        return marketbridge_points(args, symbol, start_ms, end_ms), "marketbridge"
    except RuntimeError as error:
        if args.no_fallback:
            raise
        print(f"{symbol}: MarketBridge request failed, falling back to Binance REST: {error}", file=sys.stderr)
        return binance_points(args, symbol, start_ms, end_ms), "binance_direct"


def load_series(args, symbols):
    end_ms = int(time.time() * 1000)
    start_ms = int((time.time() - args.days * 86_400) * 1000)
    series = {}
    sources = set()

    for symbol in symbols:
        points, source = load_symbol_points(args, symbol, start_ms, end_ms)
        if not points:
            print(f"{symbol}: no funding history returned", file=sys.stderr)
            continue
        series[symbol] = points
        sources.add(source)

    if not series:
        raise SystemExit("No funding history returned")

    source_label = sources.pop() if len(sources) == 1 else "mixed"
    return series, source_label


def write_csv(path, series):
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(["symbol", "time_ms", "time_local", "funding_pct"])
        for symbol, points in series.items():
            for point in points:
                writer.writerow([symbol, point.ts_ms, format_time(point.ts_ms), f"{point.funding_pct:.10f}"])


def format_time(ts_ms):
    return datetime.fromtimestamp(ts_ms / 1000).strftime("%Y-%m-%d %H:%M")


def format_short_time(ts_ms):
    return datetime.fromtimestamp(ts_ms / 1000).strftime("%m-%d %H:%M")


def format_duration(hours):
    if hours >= 48:
        return f"{hours / 24:.1f}d"
    if hours >= 1:
        return f"{hours:.1f}h"
    return f"{hours * 60:.0f}m"


def typical_interval_ms(points):
    if len(points) < 2:
        return 0
    diffs = [
        points[index].ts_ms - points[index - 1].ts_ms
        for index in range(1, len(points))
        if points[index].ts_ms > points[index - 1].ts_ms
    ]
    if not diffs:
        return 0
    diffs.sort()
    return diffs[len(diffs) // 2]


def anomaly_runs_for_symbol(symbol, points, threshold_pct, min_run_hours):
    interval_ms = typical_interval_ms(points)
    raw_runs = []
    current = []
    current_end_ms = None

    for index, point in enumerate(points):
        if point.funding_pct <= threshold_pct:
            current.append(point)
        else:
            if current:
                current_end_ms = point.ts_ms
                raw_runs.append((current, current_end_ms))
                current = []
                current_end_ms = None
    if current:
        if len(points) >= 2:
            last_interval_ms = points[-1].ts_ms - points[-2].ts_ms
        else:
            last_interval_ms = interval_ms
        current_end_ms = current[-1].ts_ms + max(0, last_interval_ms or interval_ms)
        raw_runs.append((current, current_end_ms))

    out = []
    for run, end_ms in raw_runs:
        start_ms = run[0].ts_ms
        duration_hours = max(0.0, (end_ms - start_ms) / 3_600_000)
        if duration_hours < min_run_hours:
            continue
        values = [point.funding_pct for point in run]
        out.append(
            AnomalyRun(
                symbol=symbol,
                start_ms=start_ms,
                end_ms=end_ms,
                duration_hours=duration_hours,
                points=len(run),
                min_funding_pct=min(values),
                max_funding_pct=max(values),
            )
        )
    return out


def anomaly_runs(series, threshold_pct, min_run_hours):
    return {
        symbol: anomaly_runs_for_symbol(symbol, points, threshold_pct, min_run_hours)
        for symbol, points in series.items()
    }


def write_anomalies_csv(path, runs_by_symbol, threshold_pct):
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(
            [
                "symbol",
                "threshold_pct",
                "start_ms",
                "start_local",
                "end_ms",
                "end_local",
                "duration_hours",
                "duration",
                "points",
                "min_funding_pct",
                "max_funding_pct",
            ]
        )
        for symbol, runs in runs_by_symbol.items():
            for run in runs:
                writer.writerow(
                    [
                        symbol,
                        f"{threshold_pct:.10f}",
                        run.start_ms,
                        format_time(run.start_ms),
                        run.end_ms,
                        format_time(run.end_ms),
                        f"{run.duration_hours:.4f}",
                        format_duration(run.duration_hours),
                        run.points,
                        f"{run.min_funding_pct:.10f}",
                        f"{run.max_funding_pct:.10f}",
                    ]
                )


def nice_ticks(min_value, max_value, count):
    if count <= 1:
        return [min_value]
    if math.isclose(min_value, max_value):
        return [min_value]
    return [min_value + (max_value - min_value) * i / (count - 1) for i in range(count)]


def chart_bounds(series):
    all_points = [point for points in series.values() for point in points]
    xs = [point.ts_ms for point in all_points]
    ys = [point.funding_pct for point in all_points]
    x_min, x_max = min(xs), max(xs)
    y_min, y_max = min(ys), max(ys)
    y_pad = max((y_max - y_min) * 0.16, 0.01)
    y_min -= y_pad
    y_max += y_pad
    if y_min > 0:
        y_min = 0.0
    if y_max < 0:
        y_max = 0.0
    return x_min, x_max, y_min, y_max


def append_chart(
    parts,
    series,
    source,
    days,
    title,
    x0,
    y0,
    width,
    height,
    show_legend=True,
    runs_by_symbol=None,
    threshold_pct=None,
):
    left, right, top, bottom = 70, 28, 64, 62
    plot_width = width - left - right
    plot_height = height - top - bottom

    x_min, x_max, y_min, y_max = chart_bounds(series)

    def sx(value):
        return x0 + left + (value - x_min) / max(1, x_max - x_min) * plot_width

    def sy(value):
        return y0 + top + (y_max - value) / max(1e-12, y_max - y_min) * plot_height

    zero_y = sy(0.0)
    y_ticks = nice_ticks(y_min, y_max, 6)
    x_ticks = nice_ticks(x_min, x_max, 6)

    parts.append(f'  <rect x="{x0}" y="{y0}" width="{width}" height="{height}" fill="#ffffff"/>\n')
    parts.append(
        f'  <text x="{x0 + left}" y="{y0 + 28}" font-family="Arial, sans-serif" '
        f'font-size="18" font-weight="700" fill="#111827">{escape(title)}</text>\n'
    )
    parts.append(
        f'  <text x="{x0 + width - right}" y="{y0 + 28}" font-family="Arial, sans-serif" '
        f'font-size="11" text-anchor="end" fill="#6b7280">{days:g} days · {source}</text>\n'
    )

    if show_legend:
        legend_x = x0 + left
        legend_y = y0 + 50
        for index, symbol in enumerate(series.keys()):
            color = COLORS[index % len(COLORS)]
            x = legend_x + index * 132
            parts.append(f'  <line x1="{x}" y1="{legend_y}" x2="{x + 22}" y2="{legend_y}" stroke="{color}" stroke-width="3"/>\n')
            parts.append(f'  <text x="{x + 28}" y="{legend_y + 4}" font-family="Arial, sans-serif" font-size="12" fill="#374151">{escape(symbol)}</text>\n')

    if runs_by_symbol:
        plot_top = y0 + top
        plot_bottom = y0 + height - bottom
        for symbol, runs in runs_by_symbol.items():
            if symbol not in series:
                continue
            for run in runs:
                run_x1 = max(x0 + left, sx(run.start_ms))
                run_x2 = min(x0 + width - right, sx(run.end_ms))
                if run_x2 <= run_x1:
                    continue
                parts.append(
                    f'  <rect x="{run_x1:.2f}" y="{plot_top}" width="{run_x2 - run_x1:.2f}" '
                    f'height="{plot_bottom - plot_top}" fill="#ef4444" opacity="0.11"/>\n'
                )
                label = (
                    f'{format_short_time(run.start_ms)} -> {format_short_time(run.end_ms)} · '
                    f'{format_duration(run.duration_hours)}'
                )
                label_x = (run_x1 + run_x2) / 2
                parts.append(
                    f'  <text x="{label_x:.2f}" y="{plot_top + 14}" font-family="Arial, sans-serif" '
                    f'font-size="10" text-anchor="middle" fill="#b91c1c">{escape(label)}</text>\n'
                )

    if threshold_pct is not None and y_min <= threshold_pct <= y_max:
        threshold_y = sy(threshold_pct)
        parts.append(
            f'  <line x1="{x0 + left}" y1="{threshold_y:.2f}" x2="{x0 + width - right}" '
            f'y2="{threshold_y:.2f}" stroke="#ef4444" stroke-width="1.2" stroke-dasharray="5 4"/>\n'
        )
        parts.append(
            f'  <text x="{x0 + width - right}" y="{threshold_y - 5:.2f}" font-family="Arial, sans-serif" '
            f'font-size="10" text-anchor="end" fill="#b91c1c">threshold {threshold_pct:.3f}%</text>\n'
        )

    for tick in y_ticks:
        y = sy(tick)
        parts.append(f'  <line x1="{x0 + left}" y1="{y:.2f}" x2="{x0 + width - right}" y2="{y:.2f}" stroke="#e5e7eb" stroke-width="1"/>\n')
        parts.append(f'  <text x="{x0 + left - 10}" y="{y + 4:.2f}" font-family="Arial, sans-serif" font-size="11" text-anchor="end" fill="#6b7280">{tick:.3f}%</text>\n')

    for tick in x_ticks:
        x = sx(tick)
        parts.append(f'  <line x1="{x:.2f}" y1="{y0 + top}" x2="{x:.2f}" y2="{y0 + height - bottom}" stroke="#f3f4f6" stroke-width="1"/>\n')
        parts.append(f'  <text x="{x:.2f}" y="{y0 + height - 32}" font-family="Arial, sans-serif" font-size="10" text-anchor="middle" fill="#6b7280">{format_time(tick)}</text>\n')

    parts.append(
        f'  <line x1="{x0 + left}" y1="{zero_y:.2f}" x2="{x0 + width - right}" y2="{zero_y:.2f}" stroke="#111827" stroke-width="1" opacity="0.75"/>\n'
    )

    for index, (symbol, points) in enumerate(series.items()):
        color = COLORS[index % len(COLORS)]
        line_points = " ".join(f"{sx(point.ts_ms):.2f},{sy(point.funding_pct):.2f}" for point in points)
        latest_point = points[-1]
        x = sx(latest_point.ts_ms)
        y = sy(latest_point.funding_pct)
        anchor = "end" if x > x0 + width - 220 else "start"
        dx = -8 if anchor == "end" else 8
        parts.append(f'  <polyline fill="none" stroke="{color}" stroke-width="2.2" points="{line_points}"/>\n')
        parts.append(f'  <circle cx="{x:.2f}" cy="{y:.2f}" r="4.2" fill="{color}"/>\n')
        parts.append(f'  <text x="{x + dx:.2f}" y="{y - 8:.2f}" font-family="Arial, sans-serif" font-size="12" text-anchor="{anchor}" fill="{color}">{escape(symbol)} {latest_point.funding_pct:.4f}%</text>\n')


def write_svg(path, series, source, days, title, runs_by_symbol=None, threshold_pct=None):
    width, height = 1180, 520
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">\n',
        '  <rect width="100%" height="100%" fill="#ffffff"/>\n',
    ]
    append_chart(
        parts,
        series,
        source,
        days,
        title,
        0,
        0,
        width,
        height,
        show_legend=True,
        runs_by_symbol=runs_by_symbol,
        threshold_pct=threshold_pct,
    )
    parts.extend(
        [
            '  <text x="70" y="505" font-family="Arial, sans-serif" font-size="11" fill="#6b7280">Funding values are percent per funding interval. Time labels use local timezone.</text>\n',
            "</svg>\n",
        ]
    )
    path.write_text("".join(parts), encoding="utf-8")


def write_report_svg(path, series, source, days, runs_by_symbol=None, threshold_pct=None):
    symbols = list(series.keys())
    width = 1800
    panel_w = 860
    panel_h = 390
    gap = 32
    margin = 48
    header_h = 74
    combined_h = 500
    rows = math.ceil(len(symbols) / 2)
    height = header_h + combined_h + gap + rows * panel_h + max(0, rows - 1) * gap + margin
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">\n',
        '  <rect width="100%" height="100%" fill="#f8fafc"/>\n',
        f'  <text x="{margin}" y="42" font-family="Arial, sans-serif" font-size="28" font-weight="700" fill="#111827">Funding Rate Report</text>\n',
        f'  <text x="{width - margin}" y="42" font-family="Arial, sans-serif" font-size="14" text-anchor="end" fill="#6b7280">{days:g} days · {source}</text>\n',
    ]
    append_chart(
        parts,
        series,
        source,
        days,
        "Combined Funding Rate",
        margin,
        header_h,
        width - margin * 2,
        combined_h,
        show_legend=True,
        threshold_pct=threshold_pct,
    )
    start_y = header_h + combined_h + gap
    for index, symbol in enumerate(symbols):
        col = index % 2
        row = index // 2
        x = margin + col * (panel_w + gap)
        y = start_y + row * (panel_h + gap)
        append_chart(
            parts,
            {symbol: series[symbol]},
            source,
            days,
            f"{symbol} Funding Rate",
            x,
            y,
            panel_w,
            panel_h,
            show_legend=False,
            runs_by_symbol={symbol: (runs_by_symbol or {}).get(symbol, [])},
            threshold_pct=threshold_pct,
        )
    parts.append("</svg>\n")
    path.write_text("".join(parts), encoding="utf-8")


def render_png(svg_path, png_path, size):
    if size <= 0:
        raise RuntimeError("--png-size must be positive")

    if shutil.which("rsvg-convert"):
        subprocess.run(
            ["rsvg-convert", "-w", str(size), "-o", str(png_path), str(svg_path)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        return

    if shutil.which("magick"):
        subprocess.run(
            ["magick", str(svg_path), "-resize", f"{size}x{size}", str(png_path)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        return

    if shutil.which("inkscape"):
        subprocess.run(
            [
                "inkscape",
                str(svg_path),
                "--export-type=png",
                f"--export-filename={png_path}",
                f"--export-width={size}",
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        return

    if shutil.which("qlmanage"):
        output_dir = png_path.parent
        produced = output_dir / f"{svg_path.name}.png"
        if produced.exists():
            produced.unlink()
        subprocess.run(
            ["qlmanage", "-t", "-s", str(size), "-o", str(output_dir), str(svg_path)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        if not produced.exists():
            raise RuntimeError(f"qlmanage did not produce {produced}")
        if png_path.exists():
            png_path.unlink()
        produced.rename(png_path)
        if shutil.which("sips"):
            svg_width, svg_height = svg_size(svg_path)
            if svg_width >= svg_height:
                crop_width = size
                crop_height = max(1, round(size * svg_height / svg_width))
            else:
                crop_height = size
                crop_width = max(1, round(size * svg_width / svg_height))
            subprocess.run(
                [
                    "sips",
                    "--cropToHeightWidth",
                    str(crop_height),
                    str(crop_width),
                    str(png_path),
                    "--out",
                    str(png_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                text=True,
            )
        return

    raise RuntimeError("No SVG-to-PNG renderer found. Install rsvg-convert, ImageMagick, or Inkscape.")


def write_outputs(
    output_dir,
    stem,
    series,
    source,
    days,
    title,
    args,
    runs_by_symbol=None,
    threshold_pct=None,
):
    csv_path = output_dir / f"{stem}.csv"
    svg_path = output_dir / f"{stem}.svg"
    write_csv(csv_path, series)
    write_svg(svg_path, series, source, days, title, runs_by_symbol, threshold_pct)

    files = [("csv", csv_path), ("svg", svg_path)]
    if not args.no_png:
        png_path = output_dir / f"{stem}.png"
        render_png(svg_path, png_path, args.png_size)
        files.append(("png", png_path))
    return files


def write_report_outputs(output_dir, stem, series, source, days, args, runs_by_symbol=None):
    svg_path = output_dir / f"{stem}.svg"
    write_report_svg(svg_path, series, source, days, runs_by_symbol, args.threshold_pct)

    files = [("svg", svg_path)]
    if not args.no_png:
        png_path = output_dir / f"{stem}.png"
        render_png(svg_path, png_path, args.png_size)
        files.append(("png", png_path))
    return files


def main():
    args = parse_args()
    if args.days <= 0:
        raise SystemExit("--days must be positive")

    symbols = parse_symbols(args)
    series, source = load_series(args, symbols)
    runs_by_symbol = anomaly_runs(series, args.threshold_pct, args.min_run_hours)

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    day_label = int(args.days) if args.days.is_integer() else f"{args.days:g}"
    written_files = []

    anomalies_path = output_dir / f"funding_anomalies_{day_label}d_threshold_{args.threshold_pct:g}.csv"
    write_anomalies_csv(anomalies_path, runs_by_symbol, args.threshold_pct)
    written_files.append(("anomalies_csv", anomalies_path))

    if len(series) == 1:
        chart_mode = "separate"
    else:
        chart_mode = args.chart_mode

    if chart_mode in ("combined", "both"):
        written_files.extend(
            write_outputs(
                output_dir,
                f"funding_compare_funding_{day_label}d",
                series,
                source,
                args.days,
                "Binance Funding Rate Comparison",
                args,
                None,
                args.threshold_pct,
            )
        )

    if chart_mode in ("separate", "both"):
        for symbol, points in series.items():
            single = {symbol: points}
            written_files.extend(
                write_outputs(
                    output_dir,
                    f"{safe_stem(symbol)}_funding_{day_label}d",
                    single,
                    source,
                    args.days,
                    f"{symbol} Binance Funding Rate",
                    args,
                    {symbol: runs_by_symbol.get(symbol, [])},
                    args.threshold_pct,
                )
            )

    if len(series) > 1 and not args.no_report:
        written_files.extend(
            write_report_outputs(
                output_dir,
                f"funding_report_{day_label}d",
                series,
                source,
                args.days,
                args,
                runs_by_symbol,
            )
        )

    print(f"source={source}")
    for kind, path in written_files:
        print(f"{kind}={path.resolve()}")
    for symbol, points in series.items():
        min_point = min(points, key=lambda point: point.funding_pct)
        max_point = max(points, key=lambda point: point.funding_pct)
        latest_point = points[-1]
        print(
            f"{symbol}: min={min_point.funding_pct:.6f}% at {format_time(min_point.ts_ms)}, "
            f"max={max_point.funding_pct:.6f}% at {format_time(max_point.ts_ms)}, "
            f"latest={latest_point.funding_pct:.6f}% at {format_time(latest_point.ts_ms)}"
        )


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(0)
