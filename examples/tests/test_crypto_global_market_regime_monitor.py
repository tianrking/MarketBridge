import unittest

from crypto_global_market_regime_monitor import classify_regime, observe


class GlobalMarketRegimeTests(unittest.TestCase):
    def test_stress_takes_priority_over_dominance(self):
        self.assertEqual(classify_regime({
            "btc_dominance_pct": 60.0,
            "market_cap_change_24h_pct": -4.0,
        }), "global_market_stress")

    def test_broad_and_btc_dominant_states_are_distinct(self):
        self.assertEqual(classify_regime({
            "btc_dominance_pct": 57.0,
            "market_cap_change_24h_pct": 1.0,
        }), "btc_dominant_risk_on")
        self.assertEqual(classify_regime({
            "btc_dominance_pct": 48.0,
            "market_cap_change_24h_pct": 4.0,
        }), "broad_market_risk_on")

    def test_missing_context_is_observe_only(self):
        self.assertEqual(classify_regime({}), "observe_only_missing_global_context")

    def test_observe_preserves_provider_data_and_limits(self):
        result = observe({
            "source": "coingecko_global",
            "coverage": "provider_snapshot",
            "data": {
                "btc_dominance_pct": 52.0,
                "market_cap_change_24h_pct": 0.2,
            },
        }, 55.0, -3.0, 3.0)
        self.assertEqual(result["regime"], "mixed_global_market_context")
        self.assertEqual(result["data"]["btc_dominance_pct"], 52.0)
        self.assertTrue(result["limitations"])


if __name__ == "__main__":
    unittest.main()
