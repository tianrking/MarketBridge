import json
import tempfile
import unittest
from pathlib import Path

from crypto_orca_whirlpool_monitor import classify_pool, summarize_signals
from crypto_orca_whirlpool_replay import load_records, summarize_records
from crypto_orca_whirlpool_response_replay import snapshot_state, summarize_records as summarize_response


class OrcaWhirlpoolTests(unittest.TestCase):
    def test_adaptive_fee_turnover_state(self):
        metrics = summarize_signals([
            {"source": "orca", "symbol": "SOLUSDC", "metric": "pool_top_turnover_24h_ratio", "value": 2.0},
            {"source": "orca", "symbol": "SOLUSDC", "metric": "pool_top_has_warning", "value": 0.0},
            {"source": "orca", "symbol": "SOLUSDC", "metric": "pool_top_adaptive_fee_enabled", "value": 1.0},
        ])[("orca", "SOLUSDC")]
        self.assertEqual(classify_pool(metrics, 1.0), "warning_or_adaptive_fee_pressure")

    def test_partial_cursor_stays_observe_only(self):
        metrics = {"pool_top_turnover_24h_ratio": 3.0, "pool_top_has_warning": 0.0,
                   "pool_top_adaptive_fee_enabled": 0.0, "pool_search_has_next_page": 1.0}
        self.assertEqual(classify_pool(metrics, 1.0), "observe_only_partial_orca_page")

    def test_replay_requires_consecutive_risk_states(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"pools": [{"source": "orca", "symbol": "SOLUSDC", "state": "warning_or_adaptive_fee_pressure"}]}},
            {"recorded_at_ms": 2, "observation": {"pools": [{"source": "orca", "symbol": "SOLUSDC", "state": "ordinary_whirlpool_state"}]}},
            {"recorded_at_ms": 3, "observation": {"pools": [{"source": "orca", "symbol": "SOLUSDC", "state": "high_turnover_whirlpool"}]}},
        ]
        result = summarize_records(records, 2)
        self.assertEqual(result["by_pool"]["orca:SOLUSDC"]["longest_stress_run"], 1)

    def test_response_replay_keeps_missing_lines_and_buckets(self):
        self.assertEqual(snapshot_state({"pools": [{"state": "warning_or_adaptive_fee_pressure"}]}),
                         "warning_or_adaptive_fee_pressure")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "records.jsonl"
            rows = [
                {"recorded_at_ms": 1, "observation": {"pools": [{"state": "warning_or_adaptive_fee_pressure"}], "price": {"price": 100.0}}},
                {"recorded_at_ms": 2, "observation": {"pools": [{"state": "ordinary_whirlpool_state"}], "price": {"price": 101.0}}},
                {"recorded_at_ms": 3, "observation": {"pools": [{"state": "ordinary_whirlpool_state"}], "price": {"price": 100.5}}},
            ]
            path.write_text("\n".join(json.dumps(row) for row in rows) + "\ninvalid\n", encoding="utf-8")
            loaded, invalid = load_records(path)
            self.assertEqual((len(loaded), invalid), (3, 1))
            result = summarize_response(loaded, 1, 1)
            self.assertEqual(result["by_state"]["warning_or_adaptive_fee_pressure"]["observations"], 1)


if __name__ == "__main__":
    unittest.main()
