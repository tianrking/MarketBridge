import unittest

from crypto_premium_funding_response_replay import (
    asof_funding,
    classify_state,
    summarize,
)


class PremiumFundingResponseTests(unittest.TestCase):
    def test_divergence_states_are_directionally_separate(self):
        self.assertEqual(
            classify_state(3.0, -2.0, 1.0, 1.0),
            "positive_premium_negative_funding_divergence",
        )
        self.assertEqual(
            classify_state(-3.0, 2.0, 1.0, 1.0),
            "negative_premium_positive_funding_divergence",
        )

    def test_stale_funding_is_not_carried_forward(self):
        self.assertIsNone(asof_funding([(100, 0.01)], 201, 100))
        self.assertEqual(asof_funding([(100, 0.01)], 199, 100), (100, 0.01))

    def test_summary_requires_control_and_minimum_divergence(self):
        rows = [
            {"state": "positive_premium_negative_funding_divergence", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "positive_premium_negative_funding_divergence", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "ordinary_premium_funding", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
            {"state": "ordinary_premium_funding", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
        ]
        self.assertEqual(
            summarize(rows, min_observations=2, min_edge_bps=100.0)["verdict"],
            "premium_funding_divergence_response_candidate",
        )


if __name__ == "__main__":
    unittest.main()
