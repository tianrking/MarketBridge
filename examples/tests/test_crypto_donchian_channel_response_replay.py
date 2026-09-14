"""Deterministic tests for Donchian channel response replay."""

import unittest

from crypto_donchian_channel_response_replay import (
    build_observations,
    donchian_features,
    donchian_series,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class DonchianChannelResponseTests(unittest.TestCase):
    def test_prior_window_excludes_current_bar(self):
        rows = [bar(index, 100.0 + index, 101.0 + index, 99.0 + index) for index in range(5)]
        features = donchian_features(rows, 4, 3)
        self.assertEqual(features["upper"], 104.0)
        self.assertEqual(features["lower"], 100.0)

    def test_first_breakout_is_separate_from_outside_persistence(self):
        rows = [bar(index, 100.0, 101.0, 99.0) for index in range(5)]
        rows.extend([bar(5, 105.0, 106.0, 104.0), bar(6, 107.0, 108.0, 106.0)])
        series = donchian_series(rows, 3)
        self.assertEqual(series[5]["state"], "bullish_breakout")
        self.assertEqual(series[6]["state"], "bullish_outside_channel")

    def test_build_observations_include_aligned_response(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(20)]
        rows[10] = bar(10, 110.0, 111.0, 109.0)
        observations = build_observations(rows, 3, 2)
        self.assertTrue(observations)
        self.assertIn("forward_min_path_return_pct", observations[0])

    def test_summary_requires_breakout_and_inside_control(self):
        observations = [
            {"state": "bullish_breakout", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "inside_channel", "forward_return_pct": 0.2,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.2},
        ]
        result = summarize(observations, 1)
        self.assertEqual(result["verdict"], "donchian_channel_response_reported")
        self.assertIn("bearish_breakout", result["by_state"])


if __name__ == "__main__":
    unittest.main()
