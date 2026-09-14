#!/usr/bin/env python3
"""Compatibility alias for the categorized liquidity-stress monitor."""
import runpy
import sys
from pathlib import Path

if __name__ == "__main__":
    target = Path(__file__).with_name("crypto_liquidity_stress_monitor.py")
    sys.argv[0] = str(target)
    runpy.run_path(str(target), run_name="__main__")
