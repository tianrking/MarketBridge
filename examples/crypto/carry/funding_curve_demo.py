#!/usr/bin/env python3
"""Category entry point for the root funding curve visualization utility."""

from pathlib import Path
import runpy
import sys


target = Path(__file__).resolve().parents[2] / "funding_curve_demo.py"
sys.argv[0] = str(target)
runpy.run_path(str(target), run_name="__main__")
