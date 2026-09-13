"""Deterministic tests for the adaptive cross-asset replay."""

import unittest

from crypto_adaptive_cross_asset_replay import adaptive_observation, evaluate_adaptive_cross_asset


def series(rows):
    return {symbol: [(index, value) for index, value in enumerate(values)]
            for symbol, values in rows.items()}


class AdaptiveCrossAssetReplayTests(unittest.TestCase):
    def test_conflicting_scores_shrink_to_neutral(self):
        aligned = [(index, {"BTCUSDT": 100 + index, "ETHUSDT": 100 - index})
                   for index in range(12)]
        item = adaptive_observation(aligned, 8, 2, 2, 2, 0.5, 1.0)
        self.assertIsNotNone(item)
        self.assertEqual(item["state"], "neutral")
        self.assertEqual(item["gross_exposure"], 0.0)

    def test_aligned_signal_can_be_net_long(self):
        aligned = [(index, {"BTCUSDT": 100 + index, "ETHUSDT": 100 + 2 * index})
                   for index in range(12)]
        item = adaptive_observation(aligned, 8, 2, 2, 2, 0.1, 0.75)
        self.assertEqual(item["state"], "net_long")
        self.assertAlmostEqual(item["gross_exposure"], 0.75)
        self.assertGreater(item["net_exposure"], 0)

    def test_evaluation_reports_state_counts_and_cost_adjusted_edge(self):
        result = evaluate_adaptive_cross_asset(
            series({"BTCUSDT": [100 + index for index in range(20)],
                    "ETHUSDT": [100 + 2 * index for index in range(20)]}),
            4, 2, 4, min_net_exposure=0.1, max_gross_exposure=1.0,
            min_observations=1, roundtrip_cost_bps=5,
        )
        self.assertGreater(result["observations"], 0)
        self.assertEqual(result["state_counts"]["net_long"], result["observations"])
        self.assertAlmostEqual(result["mean_cost_adjusted_edge_bps"],
                               result["mean_edge_bps"] - 5)


if __name__ == "__main__":
    unittest.main()
