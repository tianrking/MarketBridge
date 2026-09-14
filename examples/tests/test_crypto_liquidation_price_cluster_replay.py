import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1] / "crypto" / "microstructure"
SPEC = importlib.util.spec_from_file_location(
    "crypto_liquidation_price_cluster_replay",
    ROOT / "crypto_liquidation_price_cluster_replay.py",
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class LiquidationPriceClusterTests(unittest.TestCase):
    def test_dominant_price_band_and_share(self):
        events = [
            {"ts_ms": 1, "price": 100.00, "notional": 500.0},
            {"ts_ms": 2, "price": 100.10, "notional": 400.0},
            {"ts_ms": 3, "price": 103.00, "notional": 100.0},
        ]
        profile = MODULE.cluster_profile(events, 25.0)
        self.assertIsNotNone(profile)
        self.assertGreaterEqual(profile["cluster_share"], 0.8)
        self.assertEqual(profile["cluster_events"], 2)

    def test_cluster_observation_requires_share_and_cooldown(self):
        events = [
            {"ts_ms": 1_000, "price": 100.0, "notional": 900.0},
            {"ts_ms": 1_001, "price": 100.1, "notional": 100.0},
        ]
        candles = [(1_000, 100.0), (1_100, 100.0), (1_200, 99.0), (1_300, 98.0)]
        observations = MODULE.cluster_observations(
            events, candles, 1_000, 2, 500.0, 25.0, 0.8, 2
        )
        self.assertEqual(len(observations), 1)
        self.assertAlmostEqual(observations[0]["forward_return_pct"], -1.0)

    def test_summary_requires_edge_and_observations(self):
        candles = [(1, 100.0), (2, 101.0), (3, 102.0), (4, 103.0)]
        observations = [{"forward_abs_return_pct": 2.0, "cluster_share": 0.9}]
        summary = MODULE.summarize(observations, candles, 1, 2, 0.0)
        self.assertEqual(summary["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
