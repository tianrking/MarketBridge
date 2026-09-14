import unittest

from crypto_predicted_funding_monitor import dispersion_rows


class PredictedFundingMonitorTests(unittest.TestCase):
    def test_keeps_provider_venue_estimates_separate(self):
        rows = dispersion_rows([
            {"symbol": "BTC", "venue": "HlPerp", "funding_rate": 0.0001},
            {"symbol": "BTC", "venue": "BinPerp", "funding_rate": 0.0002},
        ], threshold_bps=0.5)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["lowest_venue"], "HlPerp")
        self.assertEqual(rows[0]["highest_venue"], "BinPerp")
        self.assertAlmostEqual(rows[0]["dispersion_bps"], 1.0)
        self.assertEqual(rows[0]["state"], "wide_predicted_funding_gap")

    def test_single_venue_estimate_is_not_a_spread(self):
        rows = dispersion_rows([
            {"symbol": "ETH", "venue": "HlPerp", "funding_rate": 0.0},
        ], threshold_bps=1.0)
        self.assertEqual(rows[0]["state"], "observe_only_single_venue_estimate")
        self.assertIsNone(rows[0]["dispersion_bps"])


if __name__ == "__main__":
    unittest.main()
