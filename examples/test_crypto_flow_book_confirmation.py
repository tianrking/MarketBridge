#!/usr/bin/env python3
"""Deterministic tests for the crypto flow/book confirmation monitor."""

import unittest

from crypto_flow_book_confirmation import classify_confirmation, flow_imbalance


class FlowBookConfirmationTests(unittest.TestCase):
    def test_flow_ratio_uses_known_taker_notional(self):
        result = flow_imbalance({"buy_notional": 750.0, "sell_notional": 250.0,
                                 "delta_notional": 500.0, "trade_count": 4})
        self.assertAlmostEqual(result["ratio"], 0.5)
        self.assertEqual(result["trade_count"], 4)

    def test_same_direction_flow_confirms_book(self):
        self.assertEqual(classify_confirmation(0.4, 0.3, 0.3, 0.2), "confirmed_bid_pressure")
        self.assertEqual(classify_confirmation(-0.4, -0.3, 0.3, 0.2), "confirmed_ask_pressure")

    def test_opposite_or_missing_flow_stays_research_only(self):
        self.assertEqual(classify_confirmation(0.4, -0.3, 0.3, 0.2), "book_flow_conflict")
        self.assertEqual(classify_confirmation(0.4, None, 0.3, 0.2),
                         "unconfirmed_book_pressure_missing_flow")
        self.assertEqual(classify_confirmation(None, 0.3, 0.3, 0.2),
                         "observe_only_missing_book")


if __name__ == "__main__":
    unittest.main()
