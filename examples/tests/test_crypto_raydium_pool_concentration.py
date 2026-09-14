import json
import tempfile
import unittest
from pathlib import Path

from crypto_raydium_pool_concentration_monitor import classify_pool, summarize_signals
from crypto_raydium_pool_concentration_replay import load_records, summarize_records
from crypto_raydium_pool_concentration_response_replay import snapshot_state, summarize_records as summarize_response


class RaydiumPoolConcentrationTests(unittest.TestCase):
    def test_signal_grouping_and_fragmented_state(self):
        signals = [
            {"source": "raydium", "symbol": "SOLUSDC", "metric": "pool_turnover_24h_page_ratio", "value": 2.0},
            {"source": "raydium", "symbol": "SOLUSDC", "metric": "pool_top_tvl_share_page", "value": 0.5},
            {"source": "raydium", "symbol": "SOLUSDC", "metric": "pool_page_coverage_ratio", "value": 1.0},
            {"source": "raydium", "symbol": "SOLUSDC", "metric": "pool_has_next_page", "value": 0.0},
        ]
        metrics = summarize_signals(signals)[("raydium", "SOLUSDC")]
        self.assertEqual(classify_pool(metrics, 1.0, 0.65, 1.0), "high_turnover_fragmented")

    def test_partial_page_is_not_promoted(self):
        metrics = {
            "pool_turnover_24h_page_ratio": 3.0,
            "pool_top_tvl_share_page": 0.3,
            "pool_page_coverage_ratio": 0.5,
            "pool_has_next_page": 1.0,
        }
        self.assertEqual(classify_pool(metrics, 1.0, 0.65, 1.0), "observe_only_partial_pool_catalog")

    def test_replay_requires_consecutive_stress(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"pools": [{"source": "raydium", "symbol": "SOLUSDC", "state": "high_turnover_fragmented"}]}},
            {"recorded_at_ms": 2, "observation": {"pools": [{"source": "raydium", "symbol": "SOLUSDC", "state": "ordinary_pool_state"}]}},
            {"recorded_at_ms": 3, "observation": {"pools": [{"source": "raydium", "symbol": "SOLUSDC", "state": "high_turnover_fragmented"}]}},
        ]
        summary = summarize_records(records, 2)
        self.assertEqual(summary["by_pool"]["raydium:SOLUSDC"]["longest_stress_run"], 1)
        self.assertEqual(summary["by_pool"]["raydium:SOLUSDC"]["verdict"],
                         "observe_only_no_persistent_raydium_pool_pressure")

    def test_response_buckets_and_invalid_lines_remain_visible(self):
        self.assertEqual(snapshot_state({"pools": [{"state": "high_turnover_concentrated"}]}),
                         "concentrated_pressure")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "records.jsonl"
            rows = [
                {"recorded_at_ms": 1, "observation": {"pools": [{"state": "high_turnover_fragmented"}], "price": {"price": 100.0}}},
                {"recorded_at_ms": 2, "observation": {"pools": [{"state": "ordinary_pool_state"}], "price": {"price": 101.0}}},
                {"recorded_at_ms": 3, "observation": {"pools": [{"state": "ordinary_pool_state"}], "price": {"price": 102.0}}},
            ]
            path.write_text("\n".join(json.dumps(row) for row in rows) + "\nnot-json\n", encoding="utf-8")
            loaded, invalid = load_records(path)
            self.assertEqual(len(loaded), 3)
            self.assertEqual(invalid, 1)
            result = summarize_response(loaded, 1, 1)
            self.assertEqual(result["aligned_forward_windows"], 2)
            self.assertEqual(result["by_state"]["fragmented_pressure"]["observations"], 1)


if __name__ == "__main__":
    unittest.main()
