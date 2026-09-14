"""Test bootstrap for the categorized Python example modules."""
from pathlib import Path
import sys

_examples = Path(__file__).resolve().parents[1]
for _path in [_examples, *_examples.glob("crypto/*"), _examples / "prediction", _examples / "weather"]:
    if _path.is_dir() and str(_path) not in sys.path:
        sys.path.insert(0, str(_path))
