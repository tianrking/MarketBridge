"""Deterministic tests for Vortex response replay."""

import unittest

from crypto_vortex_response_replay import (
    build_observations,
    summarize,
    vortex_features,
    vortex_series,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class VortexResponseTests(unittest.TestCase):
    def test_warmup_and_directional_movement_are_explicit(self):
        rows = [bar(index, 100.0 + index * 2, 101.0 + index * 2, 99.0 + index * 2)
                for index in range(10)]
        series = vortex_series(rows, 3, 0.05)
        self.assertIsNone(series[2])
        self.assertIsNotNone(series[3])
        self.assertGreater(series[3]["vi_plus"], series[3]["vi_minus"])
        self.assertEqual(series[3]["state"], "bullish_pressure")
        self.assertIsNone(vortex_features(rows, 0, 3))

    def test_balanced_spread_is_a_control(self):
        rows = [bar(index, 100.0 + (1.0 if index % 2 else -1.0)) for index in range(20)]
        feature = vortex_series(rows, 4, 100.0)[-1]
        self.assertEqual(feature["state"], "balanced")

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(15)]
        before = vortex_features(rows, 8, 4)
        rows[-1] = bar(14, 1000.0, 1001.0, 999.0)
        after = vortex_features(rows, 8, 4)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_pressure_and_control(self):
        rows = [bar(index, 100.0 + (index % 10) * 1.5) for index in range(40)]
        observations = build_observations(rows, 4, 0.05, 2)
        self.assertTrue(observations)
        self.assertIn("forward_max_path_return_pct", observations[0])
        manual = [
            {"state": "bullish_pressure", "event": "bullish_vi_cross", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "balanced", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "vortex_response_reported")
        self.assertEqual(result["vi_cross_observations"], 1)


if __name__ == "__main__":
    unittest.main()
