#!/usr/bin/env python3
"""Deterministic tests for aggregate derivatives sentiment monitor."""

import unittest

from crypto_derivatives_sentiment_monitor import summarize_signals


class DerivativesSentimentTests(unittest.TestCase):
    def test_positive_funding_and_high_ratio_are_crowding_context(self):
        result = summarize_signals([
            {"metric": "funding_rate", "value": 0.001},
            {"metric": "long_short_ratio", "value": 1.4},
            {"metric": "liquidation", "value": 2_000_000},
        ], 1.2, 0.8, 1_000_000)
        self.assertEqual(result["state"], "long_crowding_context")
        self.assertTrue(result["liquidation_activity"])

    def test_missing_positioning_data_stays_observe_only(self):
        result = summarize_signals([], 1.2, 0.8, 1_000_000)
        self.assertEqual(result["state"], "observe_only_missing_positioning_metrics")


if __name__ == "__main__":
    unittest.main()
