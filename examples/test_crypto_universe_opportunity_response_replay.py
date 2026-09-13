"""Deterministic tests for universe opportunity response replay."""

import unittest

from crypto_universe_opportunity_response_replay import summarize_records


def record(ts, candidates, prices, benchmark=100.0):
    response_prices = {symbol: {"price": value} for symbol, value in prices.items()}
    response_prices["BTCUSDT"] = {"price": benchmark}
    return {"recorded_at_ms": ts, "observation": {
        "candidates": [{"symbol": symbol} for symbol in candidates],
        "response_prices": response_prices, "benchmark_symbol": "BTCUSDT",
    }}


class UniverseOpportunityResponseReplayTests(unittest.TestCase):
    def test_candidate_basket_is_compared_with_btc(self):
        result = summarize_records([
            record(1, ["ETHUSDT", "SOLUSDT"], {"ETHUSDT": 100, "SOLUSDT": 100}, 100),
            record(2, [], {}, 101), record(3, [], {}, 102),
            record(4, [], {"ETHUSDT": 110, "SOLUSDT": 110}, 105),
        ], 2, 3, 1, 1)
        bucket = result["by_state"]["candidate_set_available"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_candidate_minus_benchmark_pct"], 5.0)
        self.assertEqual(result["verdict"], "universe_candidate_response_reported")

    def test_missing_candidate_prices_are_not_zero_filled(self):
        result = summarize_records([
            record(1, ["ETHUSDT", "SOLUSDT"], {"ETHUSDT": 100}, 100),
            record(2, [], {}, 101), record(3, [], {}, 102),
            record(4, [], {"ETHUSDT": 110}, 105),
        ], 2, 3, 2, 1)
        self.assertEqual(result["by_state"]["candidate_set_available"]["observations"], 0)
        self.assertEqual(result["by_state"]["candidate_set_missing_prices"]["snapshots"], 1)
        self.assertEqual(result["by_state"]["candidate_set_missing_prices"]["observations"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_candidate_responses")

    def test_empty_candidate_set_remains_separate(self):
        result = summarize_records([
            record(1, [], {}, 100), record(2, [], {}, 101),
            record(3, [], {}, 102), record(4, [], {}, 103),
        ], 2, 3, 1, 1)
        self.assertEqual(result["by_state"]["no_candidates"]["snapshots"], 1)
        self.assertEqual(result["by_state"]["no_candidates"]["observations"], 0)
        self.assertEqual(result["aligned_forward_windows"], 0)


if __name__ == "__main__":
    unittest.main()
