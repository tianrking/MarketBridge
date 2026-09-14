#!/usr/bin/env python3
"""Deterministic tests for the liquidity-stress observer."""

import unittest

from crypto_liquidity_stress_monitor import (
    book_metrics,
    classify_stress,
    ewma_vol_bps,
    impact_bps,
)


class LiquidityStressMonitorTests(unittest.TestCase):
    def test_target_size_impact_walks_multiple_levels(self):
        asks = [{"price": 100.0, "qty": 50.0}, {"price": 101.0, "qty": 50.0}]
        bids = [{"price": 100.0, "qty": 50.0}, {"price": 99.0, "qty": 50.0}]
        self.assertAlmostEqual(impact_bps(asks, 7_500), 33.11, places=1)
        self.assertAlmostEqual(impact_bps(bids, 7_500), 33.55, places=1)

    def test_ewma_volatility_is_per_bar_and_missing_window_is_explicit(self):
        self.assertGreater(ewma_vol_bps([100.0, 101.0, 99.0, 100.0], 0.2), 0.0)
        self.assertIsNone(ewma_vol_bps([100.0], 0.2))

    def test_stress_requires_two_available_components(self):
        metrics = book_metrics(
            {"bids": [{"price": 100.0, "qty": 100.0}, {"price": 90.0, "qty": 100.0}],
             "asks": [{"price": 102.0, "qty": 100.0}, {"price": 110.0, "qty": 100.0}]},
            15_000.0,
            2,
        )
        self.assertEqual(classify_stress(metrics, 40.0, 5.0, 2.0, 25.0), "liquidity_stress")
        incomplete = book_metrics(
            {"bids": [{"price": 100.0, "qty": 100.0}], "asks": []},
            5_000.0,
            1,
        )
        self.assertEqual(classify_stress(incomplete, None, 5.0, 2.0, 25.0),
                         "observe_only_missing_stress_inputs")
        self.assertEqual(classify_stress(None, 40.0, 5.0, 2.0, 25.0),
                         "observe_only_missing_order_book")


if __name__ == "__main__":
    unittest.main()
