import unittest

from crypto_historical_basis_replay import basis_points, observations, summarize


class HistoricalBasisReplayTests(unittest.TestCase):
    def test_basis_rows_are_sorted_and_contraction_is_point_in_time(self):
        points = basis_points({"rows": [
            {"ts_ms": 200, "basis_rate_bps": -20.0, "basis": -0.2},
            {"ts_ms": 100, "basis_rate_bps": 100.0, "basis": 1.0},
        ]})
        rows = observations(points, horizon_bars=1, extreme_threshold_bps=50.0)
        self.assertEqual(rows[0]["state"], "extreme_basis")
        self.assertEqual(rows[0]["basis_rate_contraction_bps"], 80.0)

    def test_summary_requires_ordinary_control(self):
        rows = [
            {"state": "extreme_basis", "basis_rate_contraction_bps": 30.0},
            {"state": "extreme_basis", "basis_rate_contraction_bps": 40.0},
        ]
        self.assertEqual(summarize(rows, 2, 0.0)["verdict"], "observe_only")

    def test_summary_promotes_edge_above_hurdle(self):
        rows = [
            {"state": "extreme_basis", "basis_rate_contraction_bps": 30.0},
            {"state": "extreme_basis", "basis_rate_contraction_bps": 40.0},
            {"state": "ordinary_basis", "basis_rate_contraction_bps": 5.0},
            {"state": "ordinary_basis", "basis_rate_contraction_bps": 5.0},
        ]
        self.assertEqual(
            summarize(rows, 2, 10.0)["verdict"],
            "historical_basis_contraction_candidate",
        )


if __name__ == "__main__":
    unittest.main()
