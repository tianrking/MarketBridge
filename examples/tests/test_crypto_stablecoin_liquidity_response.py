"""Deterministic tests for stablecoin supply response replay."""

import unittest

from crypto_stablecoin_liquidity_response_replay import summarize_records
from crypto_stablecoin_liquidity_monitor import summarize


def data(change):
    return {"assets": [{"symbol": "USDT", "change_7d_pct": change}],
            "total_supply_usd": 100.0, "chains": []}


def record(ts, price, stablecoins):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "stablecoins": stablecoins,
    }}


class StablecoinLiquidityResponseTests(unittest.TestCase):
    def test_monitor_classifies_supply_regimes(self):
        self.assertEqual(summarize(data(2.0), 1.0)["state"], "stablecoin_supply_expansion")
        self.assertEqual(summarize(data(-2.0), 1.0)["state"], "stablecoin_supply_contraction")
        self.assertEqual(summarize(data(0.2), 1.0)["state"], "stablecoin_supply_flat")

    def test_replay_compares_expansion_and_contraction(self):
        rows = [
            record(1, 100, data(2.0)),
            record(2, 101, data(0.2)),
            record(3, 102, data(0.2)),
            record(4, 110, data(0.2)),
        ]
        result = summarize_records(rows, 3, 1.0, 1)
        self.assertEqual(result["by_state"]["stablecoin_supply_expansion"]["observations"], 1)
        self.assertEqual(result["verdict"], "stablecoin_liquidity_response_reported")
        self.assertAlmostEqual(
            result["by_state"]["stablecoin_supply_expansion"]["mean_absolute_forward_return_pct"],
            10.0,
        )

    def test_replay_stays_observe_only_without_price_alignment(self):
        rows = [record(1, None, data(2.0)), record(2, 101, data(0.2)),
                record(3, 102, data(0.2)), record(4, 103, data(0.2))]
        result = summarize_records(rows, 3, 1.0, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_supply_regime_observations")


if __name__ == "__main__":
    unittest.main()
