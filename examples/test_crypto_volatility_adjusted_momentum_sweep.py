#!/usr/bin/env python3
"""Deterministic tests for the momentum parameter sweep."""

import unittest

from crypto_volatility_adjusted_momentum_sweep import parse_int_grid, run_grid


class MomentumSweepTests(unittest.TestCase):
    def test_grid_parser_deduplicates_and_sorts(self):
        self.assertEqual(parse_int_grid("12,4,12", "lookback-bars"), [4, 12])
        with self.assertRaises(Exception):
            parse_int_grid("0,4", "lookback-bars")

    def test_grid_reports_every_combination_and_selection_warning(self):
        series = {
            "BTCUSDT": [(index, 100.0 + index) for index in range(12)],
            "ETHUSDT": [(index, 100.0 + index * 0.5) for index in range(12)],
        }
        result = run_grid(series, [2, 4], [2], [1, 2], 1, 0.0, 1, 10.0)
        self.assertEqual(result["grid_size"], 4)
        self.assertEqual(len(result["rows"]), 4)
        self.assertIn("time-held-out", result["selection_warning"])
        self.assertIsNotNone(result["best_in_sample"])
        self.assertEqual(result["best_in_sample"]["paper_cost_bps"], 10.0)


if __name__ == "__main__":
    unittest.main()
