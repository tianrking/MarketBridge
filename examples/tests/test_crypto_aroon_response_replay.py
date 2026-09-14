"""Deterministic tests for Aroon response replay."""

import unittest

from crypto_aroon_response_replay import (
    aroon_features,
    aroon_series,
    build_observations,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class AroonResponseTests(unittest.TestCase):
    def test_inclusive_window_and_warmup_are_explicit(self):
        rows = [bar(index, 100.0 + index, 101.0 + index, 99.0 + index) for index in range(5)]
        series = aroon_series(rows, 4, 70.0, 50.0)
        self.assertIsNone(series[2])
        self.assertEqual(series[3]["aroon_up"], 100.0)
        self.assertEqual(series[3]["high_bars_since"], 0)
        self.assertEqual(aroon_features(rows, 1, 4), None)

    def test_recent_high_and_low_states_are_directional(self):
        rising = [bar(index, 100.0 + index * 2, 101.0 + index * 2, 99.0 + index * 2)
                  for index in range(20)]
        falling = [bar(index, 140.0 - index * 2, 141.0 - index * 2, 139.0 - index * 2)
                   for index in range(20)]
        self.assertEqual(aroon_series(rising, 5, 70.0, 50.0)[-1]["state"],
                         "bullish_recent_extreme")
        self.assertEqual(aroon_series(falling, 5, 70.0, 50.0)[-1]["state"],
                         "bearish_recent_extreme")

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(12)]
        before = aroon_features(rows, 6, 4)
        rows[-1] = bar(11, 1000.0, 1001.0, 999.0)
        after = aroon_features(rows, 6, 4)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_extreme_and_control(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(30)]
        observations = build_observations(rows, 4, 70.0, 50.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_max_path_return_pct", observations[0])
        manual = [
            {"state": "bullish_recent_extreme", "event": "bullish_aroon_cross",
             "forward_return_pct": 1.0, "direction_aligned_return_bps": 100.0,
             "forward_absolute_return_pct": 1.0},
            {"state": "consolidation", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "aroon_response_reported")
        self.assertEqual(result["aroon_cross_observations"], 1)


if __name__ == "__main__":
    unittest.main()
