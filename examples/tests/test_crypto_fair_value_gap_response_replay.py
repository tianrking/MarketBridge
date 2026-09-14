"""Deterministic tests for fair-value-gap response replay."""

import unittest

from crypto_fair_value_gap_response_replay import (
    build_observations,
    fvg_signal,
    summarize,
    zone_status,
)


class FairValueGapResponseReplayTests(unittest.TestCase):
    def setUp(self):
        self.rows = [
            {"ts_ms": 0, "open": 98.0, "high": 100.0, "low": 95.0, "close": 99.0},
            {"ts_ms": 1, "open": 99.0, "high": 112.0, "low": 98.0, "close": 110.0},
            {"ts_ms": 2, "open": 111.0, "high": 116.0, "low": 105.0, "close": 115.0},
            {"ts_ms": 3, "open": 114.0, "high": 115.0, "low": 103.0, "close": 106.0},
            {"ts_ms": 4, "open": 106.0, "high": 107.0, "low": 99.0, "close": 101.0},
            {"ts_ms": 5, "open": 101.0, "high": 103.0, "low": 98.0, "close": 100.0},
        ]

    def test_bullish_gap_is_point_in_time_and_directional(self):
        signal = fvg_signal(self.rows, 2, 5.0, 0.50)
        self.assertEqual(signal["direction"], "bullish")
        self.assertEqual(signal["zone_low"], 100.0)
        self.assertEqual(signal["zone_high"], 105.0)
        self.assertGreater(signal["gap_bps"], 5.0)

    def test_wick_fill_is_distinct_from_touch(self):
        signal = fvg_signal(self.rows, 2, 5.0, 0.50)
        self.assertEqual(zone_status(signal, [self.rows[3]]), "touched")
        self.assertEqual(zone_status(signal, [self.rows[3], self.rows[4]]), "wick_filled")

    def test_small_middle_body_is_not_a_gap_signal(self):
        rows = [
            {"ts_ms": 0, "open": 98.0, "high": 100.0, "low": 95.0, "close": 99.0},
            {"ts_ms": 1, "open": 105.0, "high": 112.0, "low": 98.0, "close": 105.1},
            {"ts_ms": 2, "open": 111.0, "high": 116.0, "low": 105.0, "close": 115.0},
        ]
        self.assertIsNone(fvg_signal(rows, 2, 5.0, 0.50))

    def test_summary_requires_event_and_ordinary_control(self):
        observations = build_observations(self.rows, 1, 5.0, 0.50)
        result = summarize(observations, 1)
        self.assertGreaterEqual(result["fvg_signals"], 1)
        self.assertGreaterEqual(result["ordinary_controls"], 1)
        self.assertEqual(result["verdict"], "fvg_response_reported")
        self.assertIn("wick_filled", result["by_zone_status"])


if __name__ == "__main__":
    unittest.main()
