"""Deterministic tests for Binance short-opportunity research levels."""

import unittest

from crypto_binance_short_opportunity_scanner import average_true_range, short_research_levels
from crypto_short_squeeze_reversal_monitor import price_structure


def candle(ts_ms, high, low, close):
    return {"ts_ms": ts_ms, "open": low, "high": high, "low": low, "close": close}


class BinanceShortOpportunityScannerTests(unittest.TestCase):
    def test_atr_requires_completed_history(self):
        rows = [candle(index, 101, 99, 100) for index in range(14)]
        self.assertIsNone(average_true_range(rows, 14))
        rows.append(candle(14, 102, 98, 99))
        self.assertGreater(average_true_range(rows, 14), 0)

    def test_levels_are_mechanical_and_not_available_without_reversal(self):
        rows = [candle(index, 100 + index, 99 + index, 99.5 + index) for index in range(20)]
        structure = price_structure(rows, lookback_bars=10)
        levels = short_research_levels(rows, structure)
        self.assertFalse(levels["available"])

    def test_confirmed_reversal_gets_entry_invalidation_and_r_targets(self):
        rows = [candle(index, 100 + index, 99 + index, 99.5 + index) for index in range(20)]
        rows[-2] = candle(18, 120, 112, 114)
        rows[-1] = candle(19, 110, 105, 106)
        structure = price_structure(rows, lookback_bars=12)
        self.assertTrue(structure["confirmed"])
        levels = short_research_levels(rows, structure)
        self.assertTrue(levels["available"])
        self.assertGreater(levels["invalidation"], levels["reference_entry"])
        self.assertLess(levels["target_1r"], levels["reference_entry"])
        self.assertLess(levels["target_2r"], levels["target_1r"])
        self.assertEqual(levels["execution"], "research_only_no_orders")


if __name__ == "__main__":
    unittest.main()
