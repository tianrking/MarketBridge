"""Deterministic tests for historical weekend reference replay."""

import unittest
from datetime import datetime
from zoneinfo import ZoneInfo

from crypto_weekend_gap_response_replay import (
    interval_millis,
    summarize,
    weekend_observations,
    weekend_pairs,
)


TZ = ZoneInfo("America/Chicago")
HOUR = 3_600_000


def ts(year, month, day, hour):
    return int(datetime(year, month, day, hour, tzinfo=TZ).timestamp() * 1000)


def candle(timestamp, open_price, close_price=None, high=None, low=None):
    close = open_price if close_price is None else close_price
    return {"ts_ms": timestamp, "open": open_price, "high": high or max(open_price, close) + 1,
            "low": low or min(open_price, close) - 1, "close": close, "volume": 10.0}


class WeekendGapResponseTests(unittest.TestCase):
    def test_interval_and_local_pairing(self):
        self.assertEqual(interval_millis("1h"), HOUR)
        self.assertIsNone(interval_millis("1M"))
        rows = [
            candle(ts(2026, 3, 6, 15), 100.0, 101.0),
            candle(ts(2026, 3, 8, 17), 105.0, 106.0),
        ]
        pairs = weekend_pairs(rows, "America/Chicago", HOUR)
        self.assertEqual(len(pairs), 1)
        self.assertEqual(pairs[0]["friday"]["close"], 101.0)
        self.assertEqual(pairs[0]["sunday"]["open"], 105.0)

    def test_large_up_dislocation_can_touch_reference(self):
        rows = [
            candle(ts(2026, 3, 6, 15), 100.0, 101.0),
            candle(ts(2026, 3, 8, 17), 105.0, 104.0),
            candle(ts(2026, 3, 8, 18), 104.0, 102.0, high=106.0, low=100.5),
            candle(ts(2026, 3, 8, 19), 102.0, 103.0),
        ]
        rows_out = weekend_observations(rows, "America/Chicago", HOUR, 2, 100.0, 5.0)
        self.assertEqual(len(rows_out), 1)
        self.assertEqual(rows_out[0]["state"], "up_dislocation")
        self.assertTrue(rows_out[0]["filled_reference"])

    def test_summary_requires_large_dislocation_and_small_control(self):
        row = {"state": "up_dislocation", "filled_reference": True,
               "forward_return_pct": -1.0, "fill_aligned_return_bps": 100.0}
        control = {"state": "small_dislocation", "filled_reference": False,
                   "forward_return_pct": 0.1, "fill_aligned_return_bps": None}
        result = summarize([row, control], 1)
        self.assertEqual(result["verdict"], "historical_weekend_dislocation_reported")
        self.assertEqual(result["dislocations"]["fill_rate"], 1.0)


if __name__ == "__main__":
    unittest.main()
