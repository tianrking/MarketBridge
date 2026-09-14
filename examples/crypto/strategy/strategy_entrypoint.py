#!/usr/bin/env python3
"""Small compatibility launcher for Python-first strategy examples."""

import runpy
import sys
from pathlib import Path

_examples = Path(__file__).resolve().parents[2]
for _path in [_examples, *_examples.glob("crypto/*"), _examples / "prediction", _examples / "weather"]:
    if _path.is_dir() and str(_path) not in sys.path:
        sys.path.insert(0, str(_path))


def run(strategy):
    examples_dir = Path(__file__).resolve().parent
    runner = examples_dir / "python_strategy_runner.py"
    sys.argv = [str(runner), "--strategy", strategy, *sys.argv[1:]]
    runpy.run_path(str(runner), run_name="__main__")
