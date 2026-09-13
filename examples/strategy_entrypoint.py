#!/usr/bin/env python3
"""Small compatibility launcher for Python-first strategy examples."""

import runpy
import sys
from pathlib import Path


def run(strategy):
    examples_dir = Path(__file__).resolve().parent
    runner = examples_dir / "python_strategy_runner.py"
    sys.argv = [str(runner), "--strategy", strategy, *sys.argv[1:]]
    runpy.run_path(str(runner), run_name="__main__")
