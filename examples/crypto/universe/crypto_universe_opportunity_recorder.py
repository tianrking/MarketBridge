#!/usr/bin/env python3
"""Category entry point for the root universe-opportunity recorder."""

import runpy
import sys
from pathlib import Path


target = Path(__file__).resolve().parents[2] / "crypto_universe_opportunity_recorder.py"
sys.argv[0] = str(target)
runpy.run_path(str(target), run_name="__main__")
