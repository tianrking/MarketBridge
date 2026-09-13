#!/usr/bin/env python3
"""Deterministic tests for crypto macro context monitor."""

import unittest

from crypto_macro_context_monitor import classify_macro, macro_rows


class MacroContextMonitorTests(unittest.TestCase):
    def test_missing_vix_stays_explicit(self):
        self.assertEqual(classify_macro({}, 25.0), "observe_only_missing_vix")

    def test_vix_threshold_labels_context_not_direction(self):
        self.assertEqual(classify_macro({"vix": {"value": 30.0}}, 25.0), "elevated_volatility_context")
        self.assertEqual(classify_macro({"vix": {"value": 15.0}}, 25.0), "normal_volatility_context")

    def test_macro_rows_filters_reference_sources(self):
        rows = macro_rows({"quotes": [
            {"source_ref": {"source": "vix"}, "payload": {"mark": 20.0}},
            {"source_ref": {"source": "random"}, "payload": {"mark": 99.0}},
        ]})
        self.assertEqual(rows["vix"]["value"], 20.0)
        self.assertNotIn("random", rows)


if __name__ == "__main__":
    unittest.main()
