#!/usr/bin/env python3
"""Deterministic tests for the crypto universe opportunity scan."""

import unittest

from crypto_universe_opportunity_scan import funding_map, rank_candidates


class UniverseOpportunityTests(unittest.TestCase):
    def test_funding_map_withholds_hourly_rate_without_interval(self):
        mapped = funding_map({"funding": [
            {"symbol": "BTCUSDT", "exchange": "binance", "funding_rate": 0.001,
             "funding_interval_ms": 28_800_000},
            {"symbol": "ETHUSDT", "exchange": "binance", "funding_rate": 0.001},
        ]})
        self.assertAlmostEqual(mapped[("binance", "BTCUSDT")]["funding_hourly_pct"], 0.0125)
        self.assertIsNone(mapped[("binance", "ETHUSDT")]["funding_hourly_pct"])

    def test_rank_requires_joined_evidence_and_preserves_gaps(self):
        volume = {"rows": [{"exchange": "binance", "market": "perp", "symbol": "BTCUSDT",
                             "quote_volume": 2_000_000.0}]}
        volatility = {"rows": [{"exchange": "binance", "symbol": "BTCUSDT",
                                 "realized_volatility": 0.25}]}
        funding = {"funding": [{"exchange": "binance", "symbol": "BTCUSDT",
                                 "funding_rate": 0.002, "funding_interval_ms": 28_800_000}]}
        candidates = rank_candidates(volume, volatility, funding, "binance", 1_000_000, 0.2, 0.01, 3)
        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0]["score"], 3)
        self.assertIn("funding_magnitude_above_threshold", candidates[0]["evidence"])

    def test_zero_values_never_count_as_universe_evidence(self):
        volume = {"rows": [{"exchange": "binance", "market": "perp", "symbol": "BTCUSDT",
                             "quote_volume": 0.0}]}
        volatility = {"rows": [{"exchange": "binance", "symbol": "BTCUSDT",
                                 "realized_volatility": 0.0}]}
        funding = {"funding": [{"exchange": "binance", "symbol": "BTCUSDT",
                                 "funding_rate": 0.0, "funding_interval_ms": 28_800_000}]}
        self.assertEqual(rank_candidates(volume, volatility, funding, "binance", 0, 0, 0, 1), [])


if __name__ == "__main__":
    unittest.main()
