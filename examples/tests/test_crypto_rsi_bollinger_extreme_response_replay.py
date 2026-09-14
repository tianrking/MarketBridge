"""Deterministic tests for RSI/Bollinger extreme response replay."""

import unittest

from crypto_rsi_bollinger_extreme_response_replay import (
    bollinger,
    build_observations,
    classify_state,
    simple_rsi,
    summarize,
)


class RsiBollingerExtremeResponseReplayTests(unittest.TestCase):
    def test_indicators_use_only_current_and_prior_closes(self):
        closes = [100.0, 101.0, 102.0, 101.0, 100.0, 99.0]
        self.assertAlmostEqual(simple_rsi(closes, 4, 3), 33.3333333333)
        bands = bollinger(closes, 4, 3, 2.0)
        self.assertEqual(bands["middle"], 101.0)

    def test_joint_states_are_separate_from_single_indicator_extremes(self):
        bands = {"upper": 100.0, "lower": 90.0, "middle": 95.0, "bandwidth_pct": 10.0}
        self.assertEqual(classify_state(80.0, bands, 105.0, 70.0, 30.0), "overbought_confluence")
        self.assertEqual(classify_state(25.0, bands, 85.0, 70.0, 30.0), "oversold_confluence")
        self.assertEqual(classify_state(75.0, bands, 95.0, 70.0, 30.0), "rsi_extreme_only")
        self.assertEqual(classify_state(50.0, bands, 105.0, 70.0, 30.0), "band_extreme_only")

    def test_summary_contains_joint_and_control_buckets(self):
        rows = [{"state": "overbought_confluence", "forward_return_pct": -1.0,
                 "forward_absolute_return_pct": 1.0, "forward_min_path_return_pct": -1.5},
                {"state": "ordinary", "forward_return_pct": 0.5,
                 "forward_absolute_return_pct": 0.5, "forward_min_path_return_pct": -0.2}]
        result = summarize(rows, 2)
        self.assertEqual(result["verdict"], "rsi_bollinger_response_reported")
        self.assertEqual(result["by_state"]["overbought_confluence"]["negative_forward_fraction"], 1.0)


if __name__ == "__main__":
    unittest.main()
