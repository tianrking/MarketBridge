"""Deterministic tests for ADX/DMI response replay."""

import unittest

from crypto_adx_dmi_response_replay import (
    build_observations,
    dmi_features,
    dmi_series,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class AdxDmiResponseTests(unittest.TestCase):
    def test_wilder_adx_warmup_is_explicit(self):
        rows = [bar(index, 100.0 + index) for index in range(8)]
        series = dmi_series(rows, 3, 20.0)
        self.assertIsNone(series[3])
        self.assertIsNotNone(series[4])
        self.assertGreaterEqual(series[4]["adx"], 0.0)
        self.assertEqual(dmi_features(rows, 1, 3, 20.0), None)

    def test_rising_and_falling_series_keep_direction_separate(self):
        rising = [bar(index, 100.0 + index * 2, 101.0 + index * 2, 99.0 + index * 2)
                  for index in range(20)]
        falling = [bar(index, 140.0 - index * 2, 141.0 - index * 2, 139.0 - index * 2)
                   for index in range(20)]
        self.assertEqual(dmi_series(rising, 3, 10.0)[-1]["state"], "strong_bullish")
        self.assertEqual(dmi_series(falling, 3, 10.0)[-1]["state"], "strong_bearish")

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(20)]
        before = dmi_features(rows, 8, 3, 20.0)
        rows[-1] = bar(19, 1000.0, 1001.0, 999.0)
        after = dmi_features(rows, 8, 3, 20.0)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_strong_and_range_control(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(30)]
        observations = build_observations(rows, 3, 20.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_min_path_return_pct", observations[0])
        manual = [
            {"state": "strong_bullish", "event": "bullish_di_cross", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "range_or_mixed", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "adx_dmi_response_reported")
        self.assertEqual(result["weak_or_range_control_observations"], 1)
        self.assertEqual(result["di_cross_observations"], 1)


if __name__ == "__main__":
    unittest.main()
