#!/usr/bin/env python3
"""Deterministic tests for the universe data-quality risk monitor."""

import unittest

from crypto_universe_delist_risk_monitor import summarize


class DelistRiskMonitorTests(unittest.TestCase):
    def test_high_and_medium_rows_require_review(self):
        result = summarize([
            {"risk": "high", "reason": "missing_current_quote"},
            {"risk": "medium", "reason": "stale_current_quote"},
            {"risk": "low", "reason": "history_and_quote_present"},
        ])
        self.assertEqual(result["high_risk_rows"], 1)
        self.assertEqual(result["medium_risk_rows"], 1)
        self.assertEqual(result["verdict"], "data_quality_review_required")

    def test_clean_rows_are_not_promoted_to_risk(self):
        result = summarize([{ "risk": "low", "reason": "history_and_quote_present" }])
        self.assertEqual(result["verdict"], "no_stale_or_missing_quote_rows")
        self.assertEqual(result["high_risk_rows"], 0)


if __name__ == "__main__":
    unittest.main()
