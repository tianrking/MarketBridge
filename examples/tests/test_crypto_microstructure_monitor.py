#!/usr/bin/env python3
"""Deterministic tests for the crypto microstructure monitor."""

import unittest

from crypto_microstructure_monitor import classify_signal, imbalance


class MicrostructureMonitorTests(unittest.TestCase):
    def test_notional_imbalance_uses_top_levels(self):
        result = imbalance(
            [{"price": 100.0, "qty": 10.0}, {"price": 99.0, "qty": 1.0}],
            [{"price": 100.0, "qty": 5.0}, {"price": 101.0, "qty": 1.0}],
            1,
        )
        self.assertAlmostEqual(result["imbalance"], 1.0 / 3.0)
        self.assertAlmostEqual(result["bid_depth_notional"], 1000.0)
        self.assertAlmostEqual(result["ask_depth_notional"], 500.0)

    def test_funding_conflict_is_not_silently_promoted(self):
        self.assertEqual(
            classify_signal(0.4, "long_crowded_warning", 0.3),
            "bid_pressure_with_long_crowding_conflict",
        )
        self.assertEqual(classify_signal(-0.4, "normal", 0.3), "ask_pressure_candidate")
        self.assertEqual(classify_signal(None, "normal", 0.3), "observe_only_missing_book")


if __name__ == "__main__":
    unittest.main()
