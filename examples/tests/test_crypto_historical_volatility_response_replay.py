import unittest

from crypto_historical_volatility_response_replay import classify_state, summarize, volatility_points


class HistoricalVolatilityResponseTests(unittest.TestCase):
    def test_provider_volatility_buckets_are_explicit(self):
        self.assertEqual(classify_state(60.0, 25.0, 50.0), "high_provider_historical_volatility")
        self.assertEqual(classify_state(20.0, 25.0, 50.0), "low_provider_historical_volatility")
        self.assertEqual(classify_state(None, 25.0, 50.0), "observe_only_missing_volatility")

    def test_provider_rows_are_normalized_and_sorted(self):
        points = volatility_points({"rows": [
            {"ts_ms": 200, "volatility": "0.30", "period_days": 30},
            {"ts_ms": 100, "volatility": 0.50, "period_days": 30},
        ]})
        self.assertEqual(points, [(100, 50.0, 30), (200, 30.0, 30)])

    def test_summary_compares_high_vol_with_ordinary_control(self):
        rows = [
            {"state": "high_provider_historical_volatility", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "high_provider_historical_volatility", "forward_return_pct": 2.0,
             "absolute_forward_return_pct": 2.0},
            {"state": "ordinary_provider_historical_volatility", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
            {"state": "ordinary_provider_historical_volatility", "forward_return_pct": 0.5,
             "absolute_forward_return_pct": 0.5},
        ]
        self.assertEqual(
            summarize(rows, min_observations=2, min_edge_bps=100.0)["verdict"],
            "high_historical_volatility_response_candidate",
        )


if __name__ == "__main__":
    unittest.main()
