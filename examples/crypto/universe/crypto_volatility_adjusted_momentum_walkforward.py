#!/usr/bin/env python3
"""Categorized launcher for the time-ordered momentum holdout replay."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from crypto.entrypoint import run  # noqa: E402


if __name__ == "__main__":
    run("crypto_volatility_adjusted_momentum_walkforward.py")
