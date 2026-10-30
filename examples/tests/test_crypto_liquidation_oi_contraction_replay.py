import json
import tempfile
import unittest
from pathlib import Path

from crypto_liquidation_oi_contraction_replay import load_rows, summarize


class CorrectedOiContractionTests(unittest.TestCase):
    def test_load_rows_reads_corrected_v3_schema(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "events.jsonl"
            path.write_text(json.dumps({
                "E0": "2025-09-01T21:45:00Z", "klass": "contraction",
                "disp": -0.02, "doi": -0.11, "notional_W": 2_000_000,
                "p6h": -0.01, "p24h": 0.02,
                "rec_min": 90, "rec_event": True, "rec_complete": True,
            }) + "\n", encoding="utf-8")
            rows = load_rows(path)
        self.assertEqual(rows[0]["klass"], "contraction")
        self.assertEqual(rows[0]["disp"], -0.02)
        self.assertEqual(rows[0]["doi"], -0.11)

    def test_summary_compares_oi_classes_without_mechanism_claim(self):
        rows = [
            {"klass": "contraction", "disp": -0.03, "doi": -0.12,
             "notional_w": 3_000_000, "rec_min": 20, "rec_event": True},
            {"klass": "contraction", "disp": -0.02, "doi": -0.11,
             "notional_w": 2_000_000, "rec_min": 30, "rec_event": False},
            {"klass": "limited", "disp": -0.01, "doi": -0.01,
             "notional_w": 500_000, "rec_min": 60, "rec_event": True},
            {"klass": "limited", "disp": -0.01, "doi": -0.02,
             "notional_w": 400_000, "rec_min": 50, "rec_event": True},
        ]
        result = summarize(rows, min_observations=2)
        self.assertEqual(result["verdict"], "corrected_v3_descriptive_summary")
        self.assertGreater(result["contraction_minus_limited"]["median_abs_displacement_pct"], 0)
        self.assertEqual(result["by_class"]["contraction"]["observations"], 2)
        self.assertEqual(result["by_class"]["contraction"]["median_oi_change_pct"], -11.5)

        observe_only = summarize(rows[:1], min_observations=2)
        self.assertEqual(observe_only["verdict"], "observe_only_insufficient_class_sample")
