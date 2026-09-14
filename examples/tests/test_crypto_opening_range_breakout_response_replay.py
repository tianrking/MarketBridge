"""Deterministic tests for UTC opening-range breakout response replay."""

import unittest

from crypto_opening_range_breakout_response_replay import (
    build_observations,
    opening_range_features,
    opening_range_series,
    summarize,
)


DAY = 86_400_000


def bar(day, hour, close, high=None, low=None):
    return {"ts_ms": day * DAY + hour * 3_600_000, "open": close,
            "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class OpeningRangeResponseTests(unittest.TestCase):
    def test_opening_bars_have_no_signal_and_later_bar_has_range(self):
        rows = [bar(0, hour, 100.0 + hour, 101.0 + hour, 99.0 + hour)
                for hour in range(6)]
        series = opening_range_series(rows, 3, 0.0)
        self.assertIsNone(series[2])
        self.assertEqual(series[3]["opening_high"], 103.0)
        self.assertEqual(series[3]["opening_low"], 99.0)
        self.assertIsNone(opening_range_features(rows, 0))

    def test_first_breakout_is_distinct_from_persistence(self):
        rows = [bar(0, hour, 100.0, 101.0, 99.0) for hour in range(4)]
        rows.extend([bar(0, 4, 103.0, 104.0, 102.0), bar(0, 5, 104.0, 105.0, 103.0)])
        series = opening_range_series(rows, 3, 0.0)
        self.assertEqual(series[4]["state"], "bullish_breakout")
        self.assertEqual(series[5]["state"], "bullish_outside")

    def test_new_utc_day_resets_range(self):
        rows = [bar(0, hour, 100.0, 101.0, 99.0) for hour in range(4)]
        rows.extend([bar(1, hour, 200.0, 201.0, 199.0) for hour in range(4)])
        series = opening_range_series(rows, 3, 0.0)
        self.assertEqual(series[7]["opening_high"], 201.0)
        self.assertEqual(series[7]["opening_low"], 199.0)

    def test_summary_requires_breakout_and_control(self):
        manual = [
            {"state": "bullish_breakout", "event": "bullish_orb_breakout",
             "forward_return_pct": 1.0, "direction_aligned_return_bps": 100.0,
             "forward_absolute_return_pct": 1.0},
            {"state": "inside_opening_range", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "opening_range_response_reported")
        self.assertEqual(result["opening_breakout_observations"], 1)


if __name__ == "__main__":
    unittest.main()
