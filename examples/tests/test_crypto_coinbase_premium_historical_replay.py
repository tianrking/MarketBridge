"""Deterministic tests for historical Coinbase premium replay."""

import unittest

from crypto_coinbase_premium_historical_replay import historical_observations, summarize


class CoinbasePremiumHistoricalReplayTests(unittest.TestCase):
    def test_aligns_premium_and_forward_reference_return(self):
        coinbase = [(1, 101.0), (2, 102.0), (3, 103.0), (4, 110.0)]
        reference = [(1, 100.0), (2, 101.0), (3, 102.0), (4, 109.0)]
        rows = historical_observations(coinbase, reference, 3, 5.0)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["state"], "coinbase_premium")
        self.assertAlmostEqual(rows[0]["forward_return_pct"], 9.0)

    def test_summary_requires_premium_sample_and_ordinary_control(self):
        rows = [
            {"state": "coinbase_premium", "forward_return_pct": 1.0,
             "absolute_forward_return_pct": 1.0},
            {"state": "ordinary_coinbase_reference_spread", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
        ]
        result = summarize(rows, 1, 0.0)
        self.assertEqual(result["verdict"], "coinbase_premium_historical_candidate")
        self.assertAlmostEqual(result["premium_minus_ordinary_absolute_move_edge_bps"], 50.0)

    def test_summary_stays_observe_only_without_observations(self):
        result = summarize([], 1, 0.0)
        self.assertEqual(result["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
