import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "crypto_funding_regime_replay", ROOT / "crypto_funding_regime_replay.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FundingRegimeReplayTests(unittest.TestCase):
    def test_runs_split_on_neutral_and_schedule_gap(self):
        points = [
            (1_000, 0.0003, 100),
            (1_100, 0.0004, 100),
            (1_200, 0.0002, 100),
            (1_300, 0.0, 100),
            (1_400, -0.0003, 100),
            (1_500, -0.0004, 100),
            (2_000, -0.0005, 100),
        ]
        runs = MODULE.funding_runs(points, 0.0002, 3)
        self.assertEqual([run["side"] for run in runs], ["positive"])
        self.assertEqual(runs[0]["observations"], 3)

    def test_forward_return_and_summary(self):
        prices = [(1_200, 100.0), (1_300, 99.0), (1_400, 98.0), (1_500, 97.0)]
        self.assertAlmostEqual(MODULE.forward_return(1_200, prices, 2), -2.0)
        runs = [{"side": "positive", "forward_return_pct": -2.0},
                {"side": "negative", "forward_return_pct": 1.0}]
        summary = MODULE.summarize(runs)
        self.assertEqual(summary["positive"]["expected_direction_hit_rate"], 1.0)
        self.assertEqual(summary["negative"]["expected_direction_hit_rate"], 1.0)


if __name__ == "__main__":
    unittest.main()
