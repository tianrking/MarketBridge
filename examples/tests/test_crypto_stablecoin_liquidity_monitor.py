import unittest

from crypto_stablecoin_liquidity_monitor import summarize


class StablecoinLiquidityMonitorTests(unittest.TestCase):
    def test_expansion_and_contraction_states_are_explicit(self):
        data = {"total_supply_usd": 300.0, "chains": [{"name": "Ethereum"}],
                "assets": [{"change_7d_pct": 2.0}, {"change_7d_pct": 1.5}]}
        result = summarize(data, 1.0)
        self.assertEqual(result["state"], "stablecoin_supply_expansion")
        self.assertEqual(result["chain_count"], 1)
        data["assets"] = [{"change_7d_pct": -2.0}, {"change_7d_pct": -1.5}]
        self.assertEqual(summarize(data, 1.0)["state"], "stablecoin_supply_contraction")

    def test_missing_change_stays_observe_only(self):
        result = summarize({"assets": [{"symbol": "USDT"}]}, 1.0)
        self.assertEqual(result["state"], "observe_only_missing_supply_change")


if __name__ == "__main__":
    unittest.main()
