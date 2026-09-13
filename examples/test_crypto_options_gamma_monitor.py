#!/usr/bin/env python3
"""Deterministic tests for the option gamma map observer and replay."""

import unittest
from unittest.mock import patch

from crypto_options_gamma_monitor import classify_gamma, enrich_deribit_rows, summarize_expiry
from crypto_options_gamma_replay import summarize_records


class OptionsGammaTests(unittest.TestCase):
    def rows(self):
        return [
            {"payload": {"expiry_time": "2099-01-01T00:00:00Z", "strike": 100.0,
                          "option_type": "call", "gamma": 0.10, "open_interest": 10.0,
                          "underlying_price": 100.0}},
            {"payload": {"expiry_time": "2099-01-01T00:00:00Z", "strike": 100.0,
                          "option_type": "put", "gamma": 0.10, "open_interest": 5.0,
                          "underlying_price": 100.0}},
            {"payload": {"expiry_time": "2099-01-01T00:00:00Z", "strike": 120.0,
                          "option_type": "call", "gamma": 0.01, "open_interest": 2.0,
                          "underlying_price": 100.0}},
        ]

    def test_summary_reports_relative_near_spot_mass_without_dealer_sign(self):
        summary = summarize_expiry(self.rows(), "2099-01-01T00:00:00Z", 0.0, 0.05)
        self.assertEqual(summary["contracts_with_gamma"], 3)
        self.assertGreater(summary["near_spot_share"], 0.95)
        self.assertEqual(summary["dominant_strike"], 100.0)
        self.assertEqual(classify_gamma(summary, 0.5, 0.5), "near_spot_gamma_concentration")
        self.assertAlmostEqual(summary["call_gamma_mass"], 10200.0)
        self.assertAlmostEqual(summary["put_gamma_mass"], 5000.0)

    def test_missing_gamma_is_not_zero(self):
        summary = summarize_expiry([{"payload": {"strike": 100.0}}], "2099-01-01T00:00:00Z", 0.0, 0.05)
        self.assertEqual(summary["contracts_with_gamma"], 0)
        self.assertEqual(classify_gamma(summary, 0.5, 0.1), "observe_only_missing_gamma")

    def test_deribit_enrichment_is_bounded_and_visible(self):
        rows = [
            {"instrument_name": "BTC-01JAN99-100000-C", "strike": 100000.0,
             "underlying_price": 100000.0},
            {"instrument_name": "BTC-01JAN99-110000-C", "strike": 110000.0,
             "underlying_price": 100000.0},
        ]
        with patch("crypto_options_gamma_monitor.fetch", return_value={
            "book": {"gamma": 0.0001, "open_interest": 2.0,
                     "underlying_price": 100000.0}
        }) as mocked:
            enriched, coverage = enrich_deribit_rows("http://test", rows, 1, 1.0)
        self.assertEqual(mocked.call_count, 1)
        self.assertEqual(coverage["candidates_total"], 2)
        self.assertEqual(coverage["unfetched"], 1)
        self.assertEqual(enriched[0]["gamma"], 0.0001)
        self.assertIsNone(enriched[1].get("gamma"))

    def test_replay_requires_persistent_state(self):
        records = [{"observation": {"target_expiry": {
            "near_spot_share": 0.8, "concentration_at_strike": 0.3}}} for _ in range(3)]
        summary = summarize_records(records, 0.5, 0.1, 3)
        self.assertEqual(summary["verdict"], "persistent_near_spot_gamma_map")
        self.assertEqual(summary["longest_near_spot_concentration_run"], 3)


if __name__ == "__main__":
    unittest.main()
