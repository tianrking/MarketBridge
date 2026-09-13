#!/usr/bin/env python3
"""Python-first spot/perpetual carry monitor (categorised entrypoint)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from strategy_entrypoint import run  # noqa: E402


if __name__ == "__main__":
    run("basis")
