import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1] / "crypto" / "carry"
SPEC = importlib.util.spec_from_file_location(
    "funding_convergence_replay", ROOT / "funding_convergence_replay.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FundingConvergenceReplayTests(unittest.TestCase):
    def test_alignment_retains_after_cost_spread(self):
        series = {
            "low": [(1_000, 0.0001, 3_600_000)],
            "high": [(1_000, 0.0003, 3_600_000)],
        }
        rows = MODULE.aligned_spreads(series, 2.0, 0.5)
        self.assertEqual(len(rows), 1)
        self.assertAlmostEqual(rows[0]["spread_bps_per_hour"], 2.0)
        self.assertAlmostEqual(rows[0]["net_spread_bps_per_hour"], 1.5)

    def test_known_interval_age_excludes_stale_venue(self):
        series = {
            "low": [(1_000, 0.0001, 100)],
            "high": [(1_000, 0.0003, 100), (1_500, 0.0003, 100)],
        }
        rows = MODULE.aligned_spreads(series, 2.0)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["ts_ms"], 1_000)


if __name__ == "__main__":
    unittest.main()
