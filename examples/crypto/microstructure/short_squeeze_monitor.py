#!/usr/bin/env python3
"""Python-first short-squeeze research monitor."""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "strategy"))
from strategy_entrypoint import run


if __name__ == "__main__":
    run("squeeze")
