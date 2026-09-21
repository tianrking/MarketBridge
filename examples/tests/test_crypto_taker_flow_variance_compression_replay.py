import unittest

from crypto_taker_flow_variance_compression_replay import (
    compression_rows,
    forward_absolute_return,
    summarize,
)


class TakerFlowVarianceCompressionTests(unittest.TestCase):
    def test_compression_uses_only_prior_flow_windows(self):
        points = [(index, value) for index, value in enumerate(
            [0.8, -0.8, 0.7, -0.7, 0.6, -0.6, 0.1, 0.1, 0.1, 0.1], start=1)]
        rows = compression_rows(points, 4, 2, 0.2, [], 1)
        self.assertTrue(any(row["state"] == "compressed_taker_flow_variance" for row in rows))

    def test_forward_absolute_return_is_point_in_time(self):
        self.assertAlmostEqual(forward_absolute_return(2, [(1, 100.0), (2, 100.0), (3, 103.0)], 1), 3.0)

    def test_missing_forward_window_is_visible(self):
        self.assertIsNone(forward_absolute_return(2, [(2, 100.0)], 1))

    def test_summary_keeps_compressed_and_ordinary_separate(self):
        summary = summarize([
            {"state": "compressed_taker_flow_variance", "forward_absolute_return_pct": 2.0},
            {"state": "ordinary_taker_flow_variance", "forward_absolute_return_pct": 1.0},
        ])
        self.assertEqual(summary["compressed_taker_flow_variance"]["observations"], 1)
        self.assertEqual(summary["ordinary_taker_flow_variance"]["mean_forward_absolute_return_pct"], 1.0)


if __name__ == "__main__":
    unittest.main()
