import unittest

from crypto_deribit_volatility_index_response_replay import (
    build_observations,
    classify_state,
    summarize,
    volatility_points,
)


class DeribitVolatilityIndexResponseTests(unittest.TestCase):
    def test_state_boundaries_and_missing_data_are_explicit(self):
        self.assertEqual(classify_state(80.0, 25.0, 75.0), "high_deribit_volatility_index")
        self.assertEqual(classify_state(20.0, 25.0, 75.0), "low_deribit_volatility_index")
        self.assertEqual(classify_state(50.0, 25.0, 75.0), "ordinary_deribit_volatility_index")
        self.assertEqual(classify_state(None, 25.0, 75.0), "observe_only_missing_volatility_index")

    def test_points_are_sorted_and_forward_response_is_joined(self):
        points = volatility_points({"rows": [
            {"ts_ms": 200, "close": "80"},
            {"ts_ms": 100, "close": 20.0},
        ]})
        self.assertEqual(points, [(100, 20.0), (200, 80.0)])
        rows = build_observations(points, [(100, 100.0), (200, 101.0), (300, 103.0)],
                                  horizon_bars=1, low_threshold=25.0, high_threshold=75.0)
        self.assertEqual(rows[0]["state"], "low_deribit_volatility_index")
        self.assertAlmostEqual(rows[0]["forward_return_pct"], 1.0)

    def test_high_state_response_candidate_compares_ordinary_control(self):
        rows = [
            {"state": "high_deribit_volatility_index", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "high_deribit_volatility_index", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "ordinary_deribit_volatility_index", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
            {"state": "ordinary_deribit_volatility_index", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
        ]
        result = summarize(rows, min_observations=2, min_edge_bps=100.0)
        self.assertEqual(result["verdict"], "high_deribit_volatility_response_candidate")


if __name__ == "__main__":
    unittest.main()
