import unittest

from crypto_deribit_cross_asset_volatility_response_replay import (
    align_volatility_points,
    build_observations,
    classify_spread,
    summarize,
)


class DeribitCrossAssetVolatilityTests(unittest.TestCase):
    def test_alignment_uses_only_common_provider_timestamps(self):
        result = align_volatility_points([(1, 40.0), (2, 41.0)], [(2, 60.0), (3, 61.0)])
        self.assertEqual(result, [(2, 41.0, 60.0)])

    def test_spread_states_are_directional_and_explicit(self):
        self.assertEqual(classify_spread(6.0, 5.0), "eth_volatility_premium_to_btc")
        self.assertEqual(classify_spread(-6.0, 5.0), "btc_volatility_premium_to_eth")
        self.assertEqual(classify_spread(1.0, 5.0), "cross_asset_volatility_aligned")
        self.assertEqual(classify_spread(None, 5.0), "observe_only_missing_volatility_spread")

    def test_relative_response_candidate_compares_aligned_control(self):
        prices = [(100, 100.0), (200, 101.0), (300, 103.0)]
        rows = build_observations([(100, 40.0, 50.0)], prices, prices, 1, 5.0)
        self.assertEqual(rows[0]["state"], "eth_volatility_premium_to_btc")
        result = summarize([
            {"state": "eth_volatility_premium_to_btc",
             "eth_minus_btc_forward_return_pct": 2.0,
             "absolute_relative_response_pct": 2.0},
            {"state": "eth_volatility_premium_to_btc",
             "eth_minus_btc_forward_return_pct": 2.0,
             "absolute_relative_response_pct": 2.0},
            {"state": "cross_asset_volatility_aligned",
             "eth_minus_btc_forward_return_pct": 0.5,
             "absolute_relative_response_pct": 0.5},
            {"state": "cross_asset_volatility_aligned",
             "eth_minus_btc_forward_return_pct": 0.5,
             "absolute_relative_response_pct": 0.5},
        ], min_observations=2, min_edge_bps=100.0)
        self.assertEqual(result["verdict"], "eth_volatility_premium_relative_response_candidate")


if __name__ == "__main__":
    unittest.main()
