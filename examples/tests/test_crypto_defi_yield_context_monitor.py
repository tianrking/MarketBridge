import unittest

from crypto_defi_yield_context_monitor import classify_pool, summarize


class DefiYieldContextMonitorTests(unittest.TestCase):
    def test_separates_base_and_reward_dominant_yields(self):
        data = {"pools": [
            {"pool_id": "base", "chain": "Ethereum", "project": "aave",
             "symbol": "USDC", "tvl_usd": 1000, "apy_pct": 5,
             "apy_base_pct": 4, "apy_reward_pct": 1},
            {"pool_id": "reward", "chain": "Ethereum", "project": "x",
             "symbol": "USDC", "tvl_usd": 1000, "apy_pct": 10,
             "apy_base_pct": 2, "apy_reward_pct": 8},
        ]}
        result = summarize(data, 0.5)
        self.assertEqual(result["pool_count"], 2)
        self.assertEqual(result["state_counts"]["base_yield_dominant"], 1)
        self.assertEqual(result["state_counts"]["reward_dependent_yield"], 1)

    def test_missing_metrics_stays_observe_only(self):
        self.assertEqual(
            classify_pool({"pool_id": "missing", "apy_pct": None, "tvl_usd": 10}, 0.5),
            "observe_only_missing_yield_metrics",
        )

    def test_non_positive_apy_is_not_promoted(self):
        result = summarize({"pools": [{"apy_pct": 0, "tvl_usd": 100}]})
        self.assertEqual(result["state_counts"]["non_positive_apy"], 1)


if __name__ == "__main__":
    unittest.main()
