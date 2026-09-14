import unittest

from crypto_funding_band_monitor import classify_band, observe


class FundingBandMonitorTests(unittest.TestCase):
    def test_upper_and_lower_band_states_are_explicit(self):
        upper = classify_band({
            "funding_rate": 0.021,
            "funding_rate_cap": 0.025,
            "funding_rate_floor": -0.025,
        }, 0.8)
        lower = classify_band({
            "funding_rate": -0.022,
            "funding_rate_cap": 0.025,
            "funding_rate_floor": -0.025,
        }, 0.8)
        self.assertEqual(upper["state"], "near_upper_funding_cap")
        self.assertEqual(lower["state"], "near_lower_funding_floor")

    def test_missing_band_never_becomes_a_signal(self):
        result = classify_band({"funding_rate": 0.01}, 0.8)
        self.assertEqual(result["state"], "observe_only_missing_provider_band")
        self.assertIsNone(result["proximity"])

    def test_observe_keeps_provider_context(self):
        result = observe({"funding": [{
            "exchange": "binance",
            "symbol": "BTCUSDT",
            "funding_rate": 0.022,
            "funding_rate_cap": 0.025,
            "funding_rate_floor": -0.025,
            "funding_interval_ms": 28_800_000,
        }]}, "BTCUSDT", 0.8)
        self.assertEqual(len(result["band_candidates"]), 1)
        self.assertEqual(result["rows"][0]["funding_interval_ms"], 28_800_000)


if __name__ == "__main__":
    unittest.main()
