#!/usr/bin/env python3
"""Category alias matching the root crypto liquidity-stress monitor name."""

import runpy
import sys
from pathlib import Path


target = Path(__file__).resolve().parents[2] / "crypto_liquidity_stress_monitor.py"
sys.argv[0] = str(target)
runpy.run_path(str(target), run_name="__main__")
