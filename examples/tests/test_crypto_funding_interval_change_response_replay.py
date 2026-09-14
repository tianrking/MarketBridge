import unittest

from crypto_funding_interval_change_response_replay import (
    funding_points,
    interval_points,
    summarize,
)


class FundingIntervalChangeTests(unittest.TestCase):
    def test_adjacent_provider_timestamps_define_change_without_future_data(self):
        points = funding_points({"candles": [
            {"open_time_ms": 0, "close": 0.001},
            {"open_time_ms": 8 * 3_600_000, "close": 0.002},
            {"open_time_ms": 12 * 3_600_000, "close": 0.003},
            {"open_time_ms": 16 * 3_600_000, "close": 0.004},
        ]})
        rows = interval_points(points)
        self.assertEqual(rows[0]["state"], "interval_change")
        self.assertEqual(rows[0]["interval_hours"], 4.0)
        self.assertEqual(rows[1]["state"], "stable_interval")

    def test_summary_requires_change_and_stable_controls(self):
        rows = [
            {"state": "interval_change", "forward_return_pct": 2.0},
            {"state": "interval_change", "forward_return_pct": -2.0},
            {"state": "stable_interval", "forward_return_pct": 0.5},
            {"state": "stable_interval", "forward_return_pct": 0.5},
        ]
        result = summarize(rows, min_observations=2, min_abs_edge_bps=100.0)
        self.assertEqual(result["verdict"], "funding_interval_response_candidate")


if __name__ == "__main__":
    unittest.main()
