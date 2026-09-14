#!/usr/bin/env python3
"""Deterministic tests for the crypto options skew monitor."""

import unittest

from crypto_options_skew_monitor import (
    bucket_rows,
    classify_skew,
    classify_term_structure,
    summarize_expiry,
)


class OptionsSkewTests(unittest.TestCase):
    def test_moneyness_buckets_separate_puts_and_calls(self):
        rows = [
            {"strike": 100.0, "mark_iv": 50.0, "option_type": "call"},
            {"strike": 95.0, "mark_iv": 54.0, "option_type": "put"},
            {"strike": 105.0, "mark_iv": 49.0, "option_type": "call"},
            {"strike": 80.0, "mark_iv": 80.0, "option_type": "put"},
        ]
        result = bucket_rows(rows, 100.0, 0.03, 0.85, 1.15)
        self.assertEqual(result["atm"], [50.0])
        self.assertEqual(result["put_wing"], [54.0])
        self.assertEqual(result["call_wing"], [49.0])

    def test_skew_and_term_states_are_explicit(self):
        summary = summarize_expiry([
            {"payload": {"strike": 100.0, "mark_iv": 50.0, "option_type": "call",
                          "underlying_price": 100.0, "expiry_time": "2099-01-01T00:00:00Z"}},
            {"payload": {"strike": 95.0, "mark_iv": 55.0, "option_type": "put",
                          "underlying_price": 100.0, "expiry_time": "2099-01-01T00:00:00Z"}},
            {"payload": {"strike": 105.0, "mark_iv": 50.0, "option_type": "call",
                          "underlying_price": 100.0, "expiry_time": "2099-01-01T00:00:00Z"}},
        ], "2099-01-01T00:00:00Z", 0.0, 0.03, 0.85, 1.15)
        self.assertAlmostEqual(summary["put_call_skew_iv"], 5.0)
        self.assertEqual(classify_skew(summary, 3.0, 3.0), "downside_protection_demand")
        self.assertEqual(classify_term_structure(50.0, 55.0, 3.0), "upward_iv_term_structure")
        self.assertEqual(classify_term_structure(None, 55.0, 3.0), "observe_only_missing_term_points")

    def test_delta_buckets_use_provider_greeks_when_requested(self):
        rows = [
            {"mark_iv": 50.0, "delta": 0.50, "option_type": "call"},
            {"mark_iv": 60.0, "delta": -0.25, "option_type": "put"},
            {"mark_iv": 52.0, "delta": 0.25, "option_type": "call"},
            {"mark_iv": 80.0, "delta": -0.80, "option_type": "put"},
        ]
        result = bucket_rows(rows, 100.0, 0.03, 0.85, 1.15,
                             bucket_mode="delta", delta_band=0.03)
        self.assertEqual(result["atm"], [50.0])
        self.assertEqual(result["put_wing"], [60.0])
        self.assertEqual(result["call_wing"], [52.0])


if __name__ == "__main__":
    unittest.main()
