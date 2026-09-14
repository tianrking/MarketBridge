"""Deterministic tests for the Bollinger squeeze replay."""

import unittest

from crypto_bollinger_squeeze_replay import band_stats, summarize


class BollingerSqueezeReplayTests(unittest.TestCase):
    def test_bandwidth_uses_middle_band_normalization(self):
        result = band_stats([10.0, 10.0, 12.0], 2, 3, 2.0)
        self.assertAlmostEqual(result["middle"], 32.0 / 3.0)
        self.assertGreater(result["bandwidth_pct"], 0.0)

    def test_summary_applies_direction_and_paper_cost(self):
        events = [
            {"direction_sign": 1, "forward_return_pct": 1.0},
            {"direction_sign": -1, "forward_return_pct": -0.5},
        ]
        result = summarize(events, 10.0, 0.0, 2)
        self.assertEqual(result["forward_observations"], 2)
        self.assertEqual(result["directional_hit_rate"], 1.0)
        self.assertAlmostEqual(result["mean_cost_adjusted_return_bps"], 65.0)
        self.assertEqual(result["verdict"], "bollinger_squeeze_response_reported")

    def test_missing_forward_window_stays_out_of_summary(self):
        result = summarize([{"direction_sign": 1, "forward_return_pct": None}], 0, 0, 1)
        self.assertEqual(result["forward_observations"], 0)
        self.assertEqual(result["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
