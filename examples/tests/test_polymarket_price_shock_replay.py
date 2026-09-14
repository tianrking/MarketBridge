#!/usr/bin/env python3
"""Small deterministic tests for the price-shock replay helpers."""

import unittest

from polymarket_price_shock_replay import detect_shocks, numeric_points, score_shocks


class PriceShockReplayTests(unittest.TestCase):
    def test_numeric_points_discards_invalid_and_sorts(self):
        payload = {"history": [{"t": 3, "p": 0.53}, {"t": 1, "p": 0.5}, {"t": 2, "p": 2.0}]}
        self.assertEqual(numeric_points(payload), [(1.0, 0.5), (3.0, 0.53)])

    def test_shock_and_forward_continuation_are_scored(self):
        points = [(0.0, 0.50), (1.0, 0.52), (2.0, 0.53), (3.0, 0.54), (4.0, 0.53)]
        shocks = detect_shocks(points, 100.0, 0.05, 0.95, 2)
        scored = score_shocks(points, shocks, 2)
        self.assertEqual(len(scored), 1)
        self.assertEqual(scored[0]["direction"], "up")
        self.assertTrue(scored[0]["same_direction"])
        self.assertAlmostEqual(scored[0]["forward_bps"], 200.0)


if __name__ == "__main__":
    unittest.main()
