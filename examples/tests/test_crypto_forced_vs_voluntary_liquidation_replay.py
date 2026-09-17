import json
import tempfile
import unittest
from pathlib import Path

from crypto_forced_vs_voluntary_liquidation_replay import load_rows, summarize


class ForcedVsVoluntaryTests(unittest.TestCase):
    def test_load_rows_normalizes_class_aliases(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "events.jsonl"
            path.write_text(json.dumps({
                "event_time_ms": 1, "class": "forced",
                "peak_dislocation": -0.02, "ttr_min": 30,
            }) + "\n", encoding="utf-8")
            rows = load_rows(path)
        self.assertEqual(rows[0]["klass"], "deleverage")
        self.assertEqual(rows[0]["peak_disloc"], -0.02)

    def test_summary_compares_classes_and_requires_both_controls(self):
        rows = [
            {"klass": "deleverage", "peak_disloc": -0.03,
             "transitory_share": 0.8, "ttr_min": 20, "censored": False},
            {"klass": "deleverage", "peak_disloc": -0.02,
             "transitory_share": 0.7, "ttr_min": 30, "censored": False},
            {"klass": "churn", "peak_disloc": -0.01,
             "transitory_share": 0.3, "ttr_min": 60, "censored": True},
            {"klass": "churn", "peak_disloc": -0.01,
             "transitory_share": 0.4, "ttr_min": 50, "censored": False},
        ]
        result = summarize(rows, min_observations=2, permutations=50, seed=3)
        self.assertEqual(result["verdict"], "forced_vs_voluntary_response_reported")
        self.assertGreater(result["deleverage_minus_churn"]["median_peak_abs_disloc"], 0)
        self.assertEqual(result["by_class"]["deleverage"]["observations"], 2)

        observe_only = summarize(rows[:1], min_observations=2, permutations=20, seed=3)
        self.assertEqual(observe_only["verdict"], "observe_only_insufficient_class_sample")
