#!/usr/bin/env python3
"""Run a compatibility implementation from a categorised Python entrypoint."""

import runpy
import sys
from pathlib import Path


def run(script):
    examples_dir = Path(__file__).resolve().parent.parent
    target = examples_dir / script
    sys.argv = [str(target), *sys.argv[1:]]
    runpy.run_path(str(target), run_name="__main__")
