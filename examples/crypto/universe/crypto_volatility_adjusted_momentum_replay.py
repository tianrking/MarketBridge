#!/usr/bin/env python3
"""Categorized launcher for volatility-adjusted cross-asset momentum."""

import runpy
import sys
from pathlib import Path


if __name__ == "__main__":
    target = Path(__file__).resolve().parents[2] / "crypto_volatility_adjusted_momentum_replay.py"
    sys.path.insert(0, str(target.parent))
    sys.argv[0] = str(target)
    runpy.run_path(str(target), run_name="__main__")
