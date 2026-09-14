#!/usr/bin/env python3
"""Deterministic tests for Jupiter route-impact research helpers."""

import tempfile
import unittest
from pathlib import Path

from crypto_jupiter_route_impact_monitor import metric_key, summarize_route_signals
from crypto_jupiter_route_impact_replay import summarize_records


class JupiterRouteImpactTests(unittest.TestCase):
    def test_metric_key_requires_positive_amount_suffix(self):
        self.assertEqual(metric_key("route_price_impact_ratio_100", "route_price_impact_ratio"), 100)
        self.assertIsNone(metric_key("route_price_impact_ratio_bad", "route_price_impact_ratio"))

    def test_summary_groups_size_ladder_and_labels_impact(self):
        rows = [
            {"source": "jupiter", "symbol": "SOLUSDC", "metric": "route_price_impact_ratio_100", "value": 0.01},
            {"source": "jupiter", "symbol": "SOLUSDC", "metric": "route_hops_100", "value": 2},
            {"source": "jupiter", "symbol": "SOLUSDC", "metric": "route_price_impact_ratio_200", "value": 0.001},
            {"source": "jupiter", "symbol": "SOLUSDC", "metric": "route_price_200", "value": 150.0},
        ]
        result = summarize_route_signals(rows, 0.005)
        self.assertEqual(len(result), 2)
        self.assertEqual(result[0]["state"], "high_route_impact")
        self.assertEqual(result[0]["hops"], 2.0)
        self.assertEqual(result[1]["state"], "normal_route_impact")

    def test_replay_keeps_routes_separate_and_requires_run(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"routes": [
                {"source": "jupiter", "symbol": "SOLUSDC", "input_amount_atomic": 100, "state": "high_route_impact"}
            ]}},
            {"recorded_at_ms": 2, "observation": {"routes": [
                {"source": "jupiter", "symbol": "SOLUSDC", "input_amount_atomic": 100, "state": "high_route_impact"}
            ]}},
        ]
        result = summarize_records(records, 2)
        row = result["by_route"]["jupiter:SOLUSDC:100"]
        self.assertEqual(row["longest_high_impact_run"], 2)
        self.assertEqual(row["verdict"], "persistent_jupiter_route_impact_candidate")

    def test_replay_loader_keeps_invalid_lines_visible(self):
        from crypto_jupiter_route_impact_replay import load_records
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "routes.jsonl"
            path.write_text('{"observation": {"routes": []}}\nbad\n', encoding="utf-8")
            records, invalid = load_records(path)
        self.assertEqual(len(records), 1)
        self.assertEqual(invalid, 1)


if __name__ == "__main__":
    unittest.main()
