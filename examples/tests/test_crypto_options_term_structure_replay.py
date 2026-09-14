#!/usr/bin/env python3
"""Deterministic tests for options IV term-structure persistence replay."""

import unittest

from crypto_options_term_structure_replay import classify_term_structure, summarize_records


def record(slope, recorded_at_ms):
    return {
        "recorded_at_ms": recorded_at_ms,
        "observation": {"term_structure": {"atm_iv_slope_iv": slope}},
    }


class OptionsTermStructureReplayTests(unittest.TestCase):
    def test_classification_keeps_missing_points_explicit(self):
        self.assertEqual(classify_term_structure({}, 3.0), "observe_only_missing_term_points")
        self.assertEqual(classify_term_structure({"term_structure": {"atm_iv_slope_iv": 4}}, 3.0),
                         "upward_iv_term_structure")
        self.assertEqual(classify_term_structure({"term_structure": {"atm_iv_slope_iv": -4}}, 3.0),
                         "inverted_iv_term_structure")

    def test_upward_persistence_is_candidate(self):
        summary = summarize_records([record(4.0, 3), record(5.0, 1), record(3.5, 2)], 3.0, 3)
        self.assertEqual(summary["snapshots"], 3)
        self.assertEqual(summary["longest_upward_run"], 3)
        self.assertEqual(summary["verdict"], "persistent_upward_iv_term_structure_candidate")

    def test_mixed_or_missing_states_do_not_promote(self):
        summary = summarize_records([record(4.0, 1), record(None, 2), record(-4.0, 3)], 3.0, 2)
        self.assertEqual(summary["slope_observations"], 2)
        self.assertEqual(summary["verdict"], "observe_only_no_persistent_term_structure")


if __name__ == "__main__":
    unittest.main()
