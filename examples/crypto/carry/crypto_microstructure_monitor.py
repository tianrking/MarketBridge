"""Compatibility import for shared MarketBridge HTTP helpers."""
import importlib.util
import sys
from pathlib import Path
_spec = importlib.util.spec_from_file_location("_mb_microstructure_monitor", Path(__file__).resolve().parents[1] / "microstructure" / "crypto_microstructure_monitor.py")
_module = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_module)
globals().update({name: getattr(_module, name) for name in dir(_module) if not name.startswith("__")})
