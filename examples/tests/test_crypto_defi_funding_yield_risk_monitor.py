import unittest

from crypto_defi_funding_yield_risk_monitor import (
    annualized_funding_pct,
    summarize,
    summarize_funding,
)
from crypto_defi_funding_yield_risk_replay import summarize_records


class DefiFundingYieldRiskTests(unittest.TestCase):
    def test_annualizes_with_provider_interval(self):
        value = annualized_funding_pct({"funding_rate": 0.0001, "funding_interval_ms": 8 * 60 * 60 * 1000})
        self.assertAlmostEqual(value, 10.95, places=2)

    def test_negative_funding_flags_reward_dependent_pool(self):
        result = summarize(
            {"pools": [{"pool_id": "susde", "project": "ethena", "symbol": "sUSDe",
                        "tvl_usd": 1_000_000, "apy_pct": 8, "apy_base_pct": 1,
                        "apy_reward_pct": 7}]},
            {"funding": [{"exchange": "binance", "symbol": "BTCUSDT",
                           "funding_rate": -0.0001, "funding_interval_ms": 8 * 60 * 60 * 1000}]},
            ["BTCUSDT"], 0.5, 0.0,
        )
        self.assertEqual(result["funding"]["state"], "negative_funding_pressure")
        self.assertEqual(result["state_counts"]["funding_sensitive_yield_risk"], 1)

    def test_replay_requires_persistent_risk_state(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"funding": {"state": "negative_funding_pressure"},
                                                     "state_counts": {"funding_sensitive_yield_risk": 1}}},
            {"recorded_at_ms": 2, "observation": {"funding": {"state": "negative_funding_pressure"},
                                                     "state_counts": {"funding_sensitive_yield_risk": 1}}},
            {"recorded_at_ms": 3, "observation": {"funding": {"state": "near_neutral_funding"},
                                                     "state_counts": {"base_yield_dominant": 1}}},
        ]
        self.assertEqual(summarize_records(records, 2)["verdict"],
                         "persistent_funding_sensitive_yield_candidate")


if __name__ == "__main__":
    unittest.main()
