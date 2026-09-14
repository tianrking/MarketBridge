"""Deterministic tests for the ETF-flow response replay."""

import csv
import json
import tempfile
import unittest
from pathlib import Path

from crypto_etf_flow_response_replay import aligned_observations, load_flows, summarize


class EtfFlowResponseReplayTests(unittest.TestCase):
    def test_farside_style_csv_parses_parenthesized_outflow(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "flows.csv"
            with path.open("w", newline="", encoding="utf-8") as handle:
                writer = csv.writer(handle)
                writer.writerow(["Date", "Total"])
                writer.writerow(["2026-09-01", "125.5"])
                writer.writerow(["2026-09-02", "(80.0)"])
            rows, invalid = load_flows(path)
        self.assertEqual(invalid, 0)
        self.assertEqual(rows[1]["flow_musd"], -80.0)

    def test_alignment_buckets_inflow_outflow_and_ordinary(self):
        flows = [{"date": "2026-09-01", "flow_musd": 150.0},
                 {"date": "2026-09-02", "flow_musd": -150.0},
                 {"date": "2026-09-03", "flow_musd": 0.0}]
        prices = [("2026-09-01", 100.0), ("2026-09-02", 101.0),
                  ("2026-09-03", 99.0), ("2026-09-04", 100.0)]
        rows = aligned_observations(flows, prices, 100.0, 1)
        self.assertEqual([row["state"] for row in rows],
                         ["large_inflow", "large_outflow", "ordinary_flow"])
        self.assertAlmostEqual(rows[0]["forward_return_pct"], 1.0)

    def test_rolling_alignment_uses_only_complete_trailing_window(self):
        flows = [{"date": "2026-09-01", "flow_musd": 80.0},
                 {"date": "2026-09-02", "flow_musd": 140.0},
                 {"date": "2026-09-03", "flow_musd": -20.0},
                 {"date": "2026-09-04", "flow_musd": -160.0}]
        prices = [("2026-09-01", 100.0), ("2026-09-02", 101.0),
                  ("2026-09-03", 102.0), ("2026-09-04", 101.0),
                  ("2026-09-05", 100.0)]
        rows = aligned_observations(flows, prices, 100.0, 1, 2, 100.0)
        self.assertEqual([row["date"] for row in rows], ["2026-09-02", "2026-09-03", "2026-09-04"])
        self.assertEqual([row["state"] for row in rows],
                         ["rolling_inflow", "rolling_inflow", "rolling_outflow"])
        self.assertEqual(rows[0]["rolling_flow_musd"], 220.0)
        self.assertEqual(rows[-1]["flow_window_observations"], 2)

    def test_summary_is_observe_only_when_sample_is_short(self):
        result = summarize([{"state": "large_inflow", "forward_return_pct": 1.0}], 2, 10.0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_aligned_flow_days")
        self.assertAlmostEqual(result["by_state"]["large_inflow"]["mean_cost_adjusted_forward_return_pct"], 0.9)

    def test_marketbridge_jsonl_flow_is_replayable(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "flows.jsonl"
            path.write_text(json.dumps({
                "observation": {"flow": {"flow_musd": 125.0,
                                           "source_time_ms": 1_788_192_000_000,
                                           "raw": {"date": "2026-09-01"}}}
            }) + "\n", encoding="utf-8")
            rows, invalid = load_flows(path)
        self.assertEqual(invalid, 0)
        self.assertEqual(rows, [{"date": "2026-09-01", "flow_musd": 125.0}])


if __name__ == "__main__":
    unittest.main()
