#!/usr/bin/env python3
"""Deterministic tests for the sentiment-extremes monitor and replay."""

import unittest

from crypto_sentiment_extremes_monitor import summarize_signals
from crypto_sentiment_extremes_replay import summarize_records


def signal(value, timestamp=1):
    return [{
        "source": "fear_greed", "metric": "fear_greed_index", "value": value,
        "ts_ms": timestamp, "raw": {"classification": "test"},
    }]


def record(state, price, timestamp):
    return {
        "recorded_at_ms": timestamp,
        "observation": {
            "sentiment": {"state": state},
            "price": {"price": price},
        },
    }


class SentimentExtremesTests(unittest.TestCase):
    def test_thresholds_and_missing_data_are_explicit(self):
        self.assertEqual(summarize_signals(signal(20), 20, 80)["state"], "extreme_fear_context")
        self.assertEqual(summarize_signals(signal(80), 20, 80)["state"], "extreme_greed_context")
        self.assertEqual(summarize_signals([], 20, 80)["state"],
                         "observe_only_missing_or_invalid_sentiment")

    def test_contrarian_forward_windows_are_aligned(self):
        result = summarize_records([
            record("extreme_fear_context", 100, 1),
            record("neutral_sentiment_context", 105, 2),
            record("extreme_greed_context", 110, 3),
            record("neutral_sentiment_context", 100, 4),
        ], horizon_records=1, min_observations=2, paper_cost_bps=10)
        self.assertEqual(result["aligned_forward_windows"], 3)
        self.assertEqual(result["contrarian_extreme_fear"]["observations"], 1)
        self.assertEqual(result["contrarian_extreme_greed"]["observations"], 1)
        self.assertEqual(result["verdict"], "extreme_sentiment_forward_response_reported")

    def test_insufficient_extreme_observations_do_not_promote(self):
        result = summarize_records([
            record("extreme_fear_context", 100, 1),
            record("neutral_sentiment_context", 101, 2),
        ], horizon_records=1, min_observations=2, paper_cost_bps=0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_extreme_observations")


if __name__ == "__main__":
    unittest.main()
