import json
import tempfile
import unittest
from pathlib import Path

from crypto_global_market_regime_replay import load_records, summarize_records


class GlobalMarketRegimeReplayTests(unittest.TestCase):
    def test_forward_response_is_grouped_by_regime(self):
        records = []
        for index, (state, price) in enumerate([
            ("global_market_stress", 100.0),
            ("global_market_stress", 98.0),
            ("global_market_stress", 97.0),
            ("broad_market_risk_on", 100.0),
            ("broad_market_risk_on", 103.0),
        ]):
            records.append({
                "recorded_at_ms": index * 600_000,
                "observation": {"regime": state, "price": {"price": price}},
            })
        result = summarize_records(records, 1, 1, 0.0)
        self.assertEqual(result["by_regime"]["global_market_stress"]["observations"], 3)
        self.assertEqual(result["by_regime"]["broad_market_risk_on"]["observations"], 1)

    def test_invalid_json_lines_are_visible(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "records.jsonl"
            path.write_text("not-json\n" + json.dumps({"observation": {}}) + "\n", encoding="utf-8")
            records, invalid = load_records(path)
        self.assertEqual(invalid, 1)
        self.assertEqual(len(records), 1)


if __name__ == "__main__":
    unittest.main()
