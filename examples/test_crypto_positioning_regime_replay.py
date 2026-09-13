import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "crypto_positioning_regime_replay", ROOT / "crypto_positioning_regime_replay.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PositioningRegimeReplayTests(unittest.TestCase):
    def test_classification_keeps_three_dimensions(self):
        self.assertEqual(
            MODULE.classify_regime(0.02, 0.5, 1.0, 0.01, 0.1, 0.1),
            "price_up|oi_rising|funding_positive",
        )
        self.assertEqual(
            MODULE.classify_regime(-0.02, -0.5, -1.0, 0.01, 0.1, 0.1),
            "price_down|oi_falling|funding_negative",
        )

    def test_point_in_time_observation_and_forward_return(self):
        funding = [(3_000, 0.0002, 100)]
        oi = [(1_000, 100.0), (2_000, 101.0), (3_000, 102.0)]
        prices = [(1_000, 100.0), (2_000, 101.0), (3_000, 102.0),
                  (4_000, 103.0), (5_000, 104.0)]
        rows = MODULE.build_observations(funding, oi, prices, 2, 1, 0.01, 0.1, 0.1)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["regime"], "price_up|oi_rising|funding_positive")
        self.assertAlmostEqual(rows[0]["forward_return_pct"], 100.0 * (103.0 / 102.0 - 1.0))

    def test_missing_inputs_are_separate_regime(self):
        rows = MODULE.build_observations(
            [(2_000, 0.0002, 100)], [(2_000, 100.0)], [(2_000, 100.0)],
            2, 1, 0.01, 0.1, 0.1,
        )
        self.assertEqual(rows[0]["regime"], "missing_inputs")
        summary = MODULE.summarize(rows, 1)
        self.assertEqual(summary["missing_input_observations"], 1)


if __name__ == "__main__":
    unittest.main()
