#!/usr/bin/env python3
"""Python-first executable liquidity-stress research monitor."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from strategy_entrypoint import run  # noqa: E402


if __name__ == "__main__":
    run("liquidity_stress")
