"""Deterministic tests for OI/price quadrant response replay."""

import unittest

from crypto_oi_price_divergence_response_replay import (
    classify_quadrant,
    oi_change_pct,
    summarize,
)


class OiPriceDivergenceResponseReplayTests(unittest.TestCase):
    def test_quadrants_are_observable_not_position_labels(self):
        self.assertEqual(classify_quadrant(1.0, 2.0, 0.1, 0.1), "price_up_oi_rising")
        self.assertEqual(classify_quadrant(1.0, -2.0, 0.1, 0.1), "price_up_oi_falling")
        self.assertEqual(classify_quadrant(-1.0, 2.0, 0.1, 0.1), "price_down_oi_rising")
        self.assertEqual(classify_quadrant(-1.0, -2.0, 0.1, 0.1), "price_down_oi_falling")
        self.assertEqual(classify_quadrant(None, 2.0, 0.1, 0.1),
                         "observe_only_missing_price_or_oi")

    def test_oi_change_is_as_of_timestamp(self):
        points = [(10, 100.0), (20, 110.0), (30, 99.0)]
        self.assertAlmostEqual(oi_change_pct(25, points), 10.0)
        self.assertIsNone(oi_change_pct(10, points))

    def test_summary_keeps_states_separate(self):
        rows = [
            {"state": "price_up_oi_falling", "forward_return_pct": 1.0},
            {"state": "price_down_oi_rising", "forward_return_pct": -2.0},
            {"state": "flat_or_mixed", "forward_return_pct": 0.1},
        ]
        result = summarize(rows, 2)
        self.assertEqual(result["usable_observations"], 3)
        self.assertEqual(result["by_state"]["price_up_oi_falling"]["observations"], 1)
        self.assertEqual(result["verdict"], "oi_price_quadrant_response_reported")


if __name__ == "__main__":
    unittest.main()
