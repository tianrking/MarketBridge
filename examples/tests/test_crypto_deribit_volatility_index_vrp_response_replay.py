import unittest

from crypto_deribit_volatility_index_vrp_response_replay import (
    build_observations,
    classify_vrp,
    realized_volatility_at,
    summarize,
)


class DeribitVolatilityIndexVrpTests(unittest.TestCase):
    def test_vrp_states_keep_missing_and_signed_thresholds_explicit(self):
        self.assertEqual(classify_vrp(6.0, 5.0), "volatility_index_premium")
        self.assertEqual(classify_vrp(-6.0, 5.0), "realized_volatility_above_index")
        self.assertEqual(classify_vrp(1.0, 5.0), "volatility_index_and_rv_aligned")
        self.assertEqual(classify_vrp(None, 5.0), "observe_only_missing_index_or_rv")

    def test_realized_volatility_uses_only_prior_price_window(self):
        prices = [(1000, 100.0), (2000, 101.0), (3000, 100.0), (4000, 102.0)]
        aligned_prices = [(0, 100.0), (3_600_000, 101.0), (7_200_000, 100.0), (10_800_000, 102.0)]
        value = realized_volatility_at(aligned_prices, 7_200_000, rv_bars=2, interval="1h")
        self.assertIsNotNone(value)
        self.assertEqual(realized_volatility_at(prices, 1000, rv_bars=2, interval="1h"), None)

    def test_premium_response_candidate_compares_aligned_control(self):
        prices = [(0, 100.0), (3_600_000, 101.0), (7_200_000, 103.0),
                  (10_800_000, 106.0), (14_400_000, 108.0)]
        rows = build_observations(
            [(7_200_000, 80.0), (10_800_000, 80.0)], prices, rv_bars=2,
            price_interval="1h", horizon_bars=1, threshold=5.0,
        )
        self.assertEqual(rows[0]["state"], "volatility_index_premium")
        result = summarize([
            {"state": "volatility_index_premium", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "volatility_index_premium", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "volatility_index_and_rv_aligned", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
            {"state": "volatility_index_and_rv_aligned", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
        ], min_observations=2, min_edge_bps=100.0)
        self.assertEqual(result["verdict"], "volatility_index_premium_response_candidate")


if __name__ == "__main__":
    unittest.main()
