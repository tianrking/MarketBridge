#!/usr/bin/env python3
"""Deterministic tests for the short-squeeze response replay."""

import unittest

from crypto_short_squeeze_response_replay import summarize_records


def record(price, timestamp, score):
    return {"recorded_at_ms": timestamp, "observation": {
        "score": score, "price": {"price": price},
    }}


class SqueezeReplayTests(unittest.TestCase):
    def test_candidate_bucket_keeps_forward_return(self):
        result = summarize_records([
            record(100, 1, 3), record(102, 2, 1), record(105, 3, 1),
        ], 2, 1, 3, 0)
        bucket = result["by_state"]["candidate"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], 5.0)

    def test_min_score_separates_observe_only(self):
        result = summarize_records([
            record(100, 1, 2), record(101, 2, 2), record(102, 3, 2),
        ], 2, 1, 3, 0)
        self.assertEqual(result["by_state"]["candidate"]["observations"], 0)
        self.assertEqual(result["by_state"]["observe_only"]["observations"], 1)

    def test_missing_price_is_not_zero(self):
        result = summarize_records([
            record(100, 1, 3), {"recorded_at_ms": 2, "observation": {"score": 3}},
            record(102, 3, 3),
        ], 1, 1, 3, 0)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_squeeze_observations")


if __name__ == "__main__":
    unittest.main()
