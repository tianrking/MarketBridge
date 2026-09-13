#!/usr/bin/env python3
"""Deterministic tests for aggregate derivatives sentiment replay."""

import unittest

from crypto_derivatives_sentiment_replay import summarize_records


def record(state, timestamp):
    return {"recorded_at_ms": timestamp, "observation": {"state": state}}


class DerivativesSentimentReplayTests(unittest.TestCase):
    def test_persistent_crowding_requires_consecutive_snapshots(self):
        result = summarize_records([
            record("long_crowding_context", 3),
            record("long_crowding_context", 1),
            record("long_crowding_context", 2),
        ], 3)
        self.assertEqual(result["long_crowding_longest_run"], 3)
        self.assertEqual(result["verdict"], "persistent_derivatives_crowding_candidate")

    def test_mixed_states_do_not_promote(self):
        result = summarize_records([
            record("long_crowding_context", 1),
            record("mixed_or_neutral_positioning_context", 2),
            record("short_crowding_context", 3),
        ], 2)
        self.assertEqual(result["verdict"], "observe_only_no_persistent_crowding")


if __name__ == "__main__":
    unittest.main()
