"""Deterministic tests for Keltner channel response replay."""

import unittest

from crypto_keltner_channel_response_replay import (
    build_observations,
    channel_features,
    ema_values,
    summarize,
    true_range,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class KeltnerChannelResponseTests(unittest.TestCase):
    def test_true_range_and_ema_are_explicit(self):
        self.assertEqual(true_range({"high": 110.0, "low": 90.0}, 100.0), 20.0)
        self.assertEqual(ema_values([1.0, 2.0, 3.0], 2)[0], 1.0)
        self.assertAlmostEqual(ema_values([1.0, 2.0, 3.0], 2)[-1], 2.5555555556)

    def test_channel_features_detect_breakout_after_inside_bar(self):
        rows = [bar(index, 100.0, 101.0, 99.0) for index in range(12)]
        rows[-1] = bar(11, 120.0, 121.0, 119.0)
        features = channel_features(rows, 11, 3, 3, 1.0)
        self.assertEqual(features["state"], "bullish_breakout")
        self.assertTrue(features["above_channel"])

    def test_build_observations_keep_future_path_fields(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(20)]
        rows[10] = bar(10, 110.0, 111.0, 109.0)
        observations = build_observations(rows, 3, 3, 1.0, 2)
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
        self.assertEqual(result["verdict"], "keltner_channel_response_reported")
        self.assertIn("bearish_breakout", result["by_state"])


if __name__ == "__main__":
    unittest.main()
