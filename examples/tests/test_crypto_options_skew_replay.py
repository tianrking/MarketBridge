#!/usr/bin/env python3
"""Deterministic tests for crypto options skew persistence replay."""

import unittest

from crypto_options_skew_replay import skew_state, summarize_records


def record(skew, state="downside_protection_demand", term="flat_iv_term_structure"):
    return {"observation": {
        "target_expiry": {"expiry_time": "2099-01-01T00:00:00Z",
                           "put_call_skew_iv": skew, "skew_state": state},
        "term_structure": {"state": term},
    }}


class OptionsSkewReplayTests(unittest.TestCase):
    def test_replay_recomputes_state_from_threshold(self):
        self.assertEqual(skew_state(2.0, 3.0), "balanced_wing_iv")
        self.assertEqual(skew_state(-4.0, 3.0), "upside_call_demand")
        self.assertEqual(skew_state(None, 3.0), "observe_only_missing_comparable_wings")

    def test_persistence_requires_repeated_same_expiry_evidence(self):
        summary = summarize_records([record(4.0), record(5.0), record(3.5)], 3.0, 3)
        expiry = summary["by_expiry"]["2099-01-01T00:00:00Z"]
        self.assertEqual(expiry["observations"], 3)
        self.assertEqual(expiry["longest_downside_run"], 3)
        self.assertEqual(expiry["verdict"], "persistent_downside_skew_candidate")

    def test_conflicting_snapshots_do_not_become_a_signal(self):
        summary = summarize_records([
            record(5.0),
            record(-4.0, "upside_call_demand", "inverted_iv_term_structure"),
            record(None, "observe_only_missing_comparable_wings"),
        ], 3.0, 2)
        expiry = summary["by_expiry"]["2099-01-01T00:00:00Z"]
        self.assertEqual(expiry["skew_observations"], 2)
        self.assertEqual(expiry["verdict"], "observe_only_no_persistent_downside_skew")


if __name__ == "__main__":
    unittest.main()
