#!/usr/bin/env python3
"""Categorized launcher for the stablecoin depeg recorder."""

import runpy
import sys
from pathlib import Path


if __name__ == "__main__":
    target = Path(__file__).resolve().parents[2] / "crypto_stablecoin_depeg_recorder.py"
    sys.argv[0] = str(target)
    runpy.run_path(str(target), run_name="__main__")
