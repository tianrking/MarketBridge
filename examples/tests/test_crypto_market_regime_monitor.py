#!/usr/bin/env python3
"""Deterministic tests for the aggregate market-regime monitor."""

import unittest

from crypto_market_regime_monitor import research_context


class MarketRegimeMonitorTests(unittest.TestCase):
    def test_regime_maps_to_research_context_only(self):
        result = research_context({"regime": "fragmented"})
        self.assertEqual(result["research_guidance"], "require_cross_venue_freshness_and_cost_checks")
        self.assertTrue(result["research_only"])

    def test_unknown_regime_is_not_promoted(self):
        result = research_context({"regime": "new_state"})
        self.assertEqual(result["research_guidance"], "observe_only_unknown_regime")


if __name__ == "__main__":
    unittest.main()
