#!/usr/bin/env python3
"""Start and supervise the MarketBridge Python analysis worker.

MarketBridge itself remains the Rust data/API process. This helper is the one
subprocess users start afterwards: it waits for the API, starts the continuous
Binance research scanner, opens the read-only workbench, and restarts the
scanner if it exits unexpectedly.
"""

import argparse
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
import webbrowser
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCANNER = ROOT / "examples/crypto/microstructure/crypto_binance_short_opportunity_scanner.py"


def api_ready(base_url, timeout):
    request = urllib.request.Request(f"{base_url.rstrip('/')}/health")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status == 200
    except (OSError, urllib.error.URLError):
        return False


def build_parser():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--wait-secs", type=float, default=60.0)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--minimum-score", type=int, default=5)
    parser.add_argument("--candle-workers", type=int, default=8)
    parser.add_argument("--signal-file", default="work/binance-short-opportunities.jsonl")
    parser.add_argument("--webhook-url", default=None)
    parser.add_argument("--no-browser", action="store_true")
    return parser


def main():
    args = build_parser().parse_args()
    if not SCANNER.is_file():
        raise SystemExit(f"scanner not found: {SCANNER}")
    if args.wait_secs <= 0 or args.interval_secs < 0 or args.limit <= 0:
        raise SystemExit("wait-secs and limit must be positive; interval-secs cannot be negative")
    deadline = time.monotonic() + args.wait_secs
    while not api_ready(args.base_url, min(5.0, args.wait_secs)):
        if time.monotonic() >= deadline:
            raise SystemExit(f"MarketBridge API did not become ready: {args.base_url}")
        time.sleep(1.0)
    command = [
        sys.executable, str(SCANNER), "--base-url", args.base_url,
        "--minimum-score", str(args.minimum_score), "--limit", str(args.limit),
        "--candle-workers", str(args.candle_workers), "--interval-secs", str(args.interval_secs),
        "--iterations", "0", "--signal-file", args.signal_file,
    ]
    if args.webhook_url:
        command.extend(["--webhook-url", args.webhook_url])
    if not args.no_browser:
        webbrowser.open(f"{args.base_url.rstrip('/')}/workbench")
    print(f"MarketBridge ready: {args.base_url}", flush=True)
    print("Analysis workbench: " + f"{args.base_url.rstrip('/')}/workbench", flush=True)
    print("Starting continuous Binance research scanner; Ctrl-C stops this supervisor.", flush=True)
    child = None
    stopping = False

    def stop(_signum, _frame):
        nonlocal stopping
        stopping = True
        if child and child.poll() is None:
            child.terminate()

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    try:
        while True:
            child = subprocess.Popen(command, cwd=ROOT)
            code = child.wait()
            if stopping:
                return 0
            if code == 0:
                return 0
            print(f"scanner exited with code {code}; restarting in 5 seconds", flush=True)
            time.sleep(5.0)
    except KeyboardInterrupt:
        stop(signal.SIGINT, None)
        return 130
    finally:
        if child and child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()


if __name__ == "__main__":
    raise SystemExit(main())
