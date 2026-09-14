#!/usr/bin/env python3
"""Deterministic tests for the crypto options VRP monitor."""

import unittest

from crypto_options_vrp_monitor import (
    annualized_realized_vol_pct,
    classify_vrp,
    interval_minutes,
)


class OptionsVrpTests(unittest.TestCase):
    def test_interval_and_annualized_realized_vol(self):
        self.assertEqual(interval_minutes("1h"), 60)
        self.assertEqual(interval_minutes("5m"), 5)
        self.assertIsNotNone(annualized_realized_vol_pct([100.0, 101.0, 100.0], "1h"))
        self.assertEqual(annualized_realized_vol_pct([100.0, 100.0], "1h"), 0.0)

    def test_vrp_states_preserve_missing_evidence(self):
        self.assertEqual(classify_vrp(7.0, 5.0), "implied_volatility_premium")
        self.assertEqual(classify_vrp(-7.0, 5.0), "realized_volatility_above_implied")
        self.assertEqual(classify_vrp(1.0, 5.0), "implied_and_realized_vol_aligned")
        self.assertEqual(classify_vrp(None, 5.0), "observe_only_missing_iv_or_rv")


if __name__ == "__main__":
    unittest.main()
