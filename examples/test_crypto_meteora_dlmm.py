import json
import tempfile
import unittest
from pathlib import Path

from crypto_meteora_dlmm_monitor import classify_pool, summarize_signals
from crypto_meteora_dlmm_replay import load_records, summarize_records
from crypto_meteora_dlmm_response_replay import snapshot_state, summarize_records as summarize_response


class MeteoraDlmmTests(unittest.TestCase):
    def test_high_fee_turnover_state(self):
        metrics = summarize_signals([
            {"source": "meteora", "symbol": "SOLUSDC", "metric": "pool_top_turnover_24h_ratio", "value": 2.0},
            {"source": "meteora", "symbol": "SOLUSDC", "metric": "pool_top_fee_tvl_ratio_24h", "value": 0.08},
            {"source": "meteora", "symbol": "SOLUSDC", "metric": "pool_top_dynamic_fee_pct", "value": 0.12},
        ])["meteora", "SOLUSDC"]
        self.assertEqual(classify_pool(metrics, 1.0, 0.05, 0.1), "high_fee_turnover_dlmm")

    def test_partial_page_stays_observe_only(self):
        metrics = {"pool_top_turnover_24h_ratio": 3.0, "pool_top_fee_tvl_ratio_24h": 0.08,
                   "pool_top_dynamic_fee_pct": 0.12, "pool_has_next_page": 1.0}
        self.assertEqual(classify_pool(metrics, 1.0, 0.05, 0.1), "observe_only_partial_meteora_page")

    def test_replay_requires_consecutive_stress(self):
        records = [{"recorded_at_ms": i, "observation": {"pools": [{"source": "meteora", "symbol": "SOLUSDC", "state": state}]}}
                   for i, state in enumerate(("high_fee_turnover_dlmm", "ordinary_dlmm_state", "high_turnover_dlmm"), 1)]
        self.assertEqual(summarize_records(records, 2)["by_pool"]["meteora:SOLUSDC"]["longest_stress_run"], 1)

    def test_response_replay_keeps_state_buckets(self):
        self.assertEqual(snapshot_state({"pools": [{"state": "high_fee_turnover_dlmm"}]}), "high_fee_turnover_dlmm")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "records.jsonl"
            rows = [{"recorded_at_ms": i, "observation": {"pools": [{"state": state}], "price": {"price": price}}}
                    for i, (state, price) in enumerate((("high_fee_turnover_dlmm", 100.0), ("ordinary_dlmm_state", 101.0), ("ordinary_dlmm_state", 100.5)), 1)]
            path.write_text("\n".join(json.dumps(row) for row in rows) + "\ninvalid\n", encoding="utf-8")
            loaded, invalid = load_records(path)
            self.assertEqual((len(loaded), invalid), (3, 1))
            self.assertEqual(summarize_response(loaded, 1, 1)["by_state"]["high_fee_turnover_dlmm"]["observations"], 1)


if __name__ == "__main__":
    unittest.main()
