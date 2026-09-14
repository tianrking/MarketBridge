"""Deterministic tests for Supertrend response replay."""

import unittest

from crypto_supertrend_response_replay import (
    build_observations,
    summarize,
    supertrend_features,
    supertrend_series,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class SupertrendResponseTests(unittest.TestCase):
    def test_atr_warmup_and_point_in_time_features(self):
        rows = [bar(index, 100.0 + index) for index in range(6)]
        series = supertrend_series(rows, 3, 2.0)
        self.assertIsNone(series[1])
        self.assertIsNotNone(series[2])
        self.assertEqual(series[2]["state"], "bullish_trend")
        self.assertEqual(supertrend_features(rows, 1, 3, 2.0), None)

    def test_uptrend_and_downtrend_are_directional(self):
        rising = [bar(index, 100.0 + index * 2, 101.0 + index * 2, 99.0 + index * 2)
                  for index in range(20)]
        falling = [bar(index, 140.0 - index * 2, 141.0 - index * 2, 139.0 - index * 2)
                   for index in range(20)]
        self.assertEqual(supertrend_series(rising, 3, 1.5)[-1]["direction_sign"], 1)
        self.assertEqual(supertrend_series(falling, 3, 1.5)[-1]["direction_sign"], -1)

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(12)]
        before = supertrend_features(rows, 6, 3, 2.0)
        rows[-1] = bar(11, 1000.0, 1001.0, 999.0)
        after = supertrend_features(rows, 6, 3, 2.0)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_flip_and_control(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(30)]
        observations = build_observations(rows, 3, 2.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_max_path_return_pct", observations[0])
        manual = [
            {"state": "bullish_flip", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "bullish_trend", "forward_return_pct": 0.2,
             "direction_aligned_return_bps": 20.0, "forward_absolute_return_pct": 0.2},
        ]
        self.assertEqual(summarize(manual, 1)["verdict"], "supertrend_response_reported")


if __name__ == "__main__":
    unittest.main()
