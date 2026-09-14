#!/usr/bin/env python3
"""Deterministic tests for the altcoin-breadth replay."""

import unittest

from crypto_altcoin_breadth_replay import breadth_observation, evaluate_breadth


def aligned_row(timestamp, btc, eth, sol, ada):
    return timestamp, {"BTCUSDT": btc, "ETHUSDT": eth, "SOLUSDT": sol, "ADAUSDT": ada}


class AltcoinBreadthTests(unittest.TestCase):
    def test_high_breadth_and_relative_outperformance(self):
        aligned = [
            aligned_row(0, 100, 100, 100, 100),
            aligned_row(1, 101, 102, 103, 102),
            aligned_row(2, 102, 106, 107, 105),
        ]
        result = breadth_observation(aligned, 1, "BTCUSDT", 1, 1, 0.25, 0.75, 2)
        self.assertEqual(result["breadth_state"], "high_altcoin_breadth")
        self.assertGreater(result["relative_edge_bps"], 0)

    def test_low_breadth_is_visible(self):
        aligned = [
            aligned_row(0, 100, 100, 100, 100),
            aligned_row(1, 105, 101, 100, 99),
            aligned_row(2, 104, 100, 99, 98),
        ]
        result = breadth_observation(aligned, 1, "BTCUSDT", 1, 1, 0.25, 0.75, 2)
        self.assertEqual(result["breadth_state"], "low_altcoin_breadth")

    def test_equal_timestamp_intersection_and_state_stats(self):
        series = {
            "BTCUSDT": [(0, 100), (1, 105), (2, 104), (3, 106)],
            "ETHUSDT": [(0, 100), (1, 102), (2, 101), (3, 107)],
            "SOLUSDT": [(0, 100), (1, 101), (2, 100), (3, 108)],
        }
        result = evaluate_breadth(series, "BTCUSDT", 1, 1, 0.25, 0.75, 2, 1, 0)
        self.assertEqual(result["aligned_points"], 4)
        self.assertGreaterEqual(result["observations"], 1)
        self.assertIn("high_altcoin_breadth", result["by_state"])

    def test_min_alt_assets_blocks_incomplete_breadth(self):
        aligned = [
            (0, {"BTCUSDT": 100, "ETHUSDT": 100}),
            (1, {"BTCUSDT": 101, "ETHUSDT": 102}),
            (2, {"BTCUSDT": 102, "ETHUSDT": 103}),
        ]
        self.assertIsNone(breadth_observation(aligned, 1, "BTCUSDT", 1, 1, 0.25, 0.75, 2))


if __name__ == "__main__":
    unittest.main()
