import importlib.util
import json
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1] / "crypto" / "universe"
SPEC = importlib.util.spec_from_file_location(
    "crypto_universe_opportunity_replay", ROOT / "crypto_universe_opportunity_replay.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class UniverseOpportunityReplayTests(unittest.TestCase):
    def _record(self, symbols):
        return {"observation": {"candidates": [{"symbol": symbol} for symbol in symbols]}}

    def test_top_k_and_symbol_persistence(self):
        records = [self._record(["BTCUSDT", "ETHUSDT"]),
                   self._record(["BTCUSDT", "SOLUSDT"]),
                   self._record(["BTCUSDT", "ETHUSDT"])]
        summary = MODULE.summarize_records(records, 1, 3)
        self.assertEqual(summary["persistent_symbols"]["BTCUSDT"], 1.0)
        self.assertEqual(summary["verdict"], "persistent_universe_candidate_set")

    def test_empty_candidates_are_not_filled(self):
        summary = MODULE.summarize_records([self._record([])] * 3, 2, 2)
        self.assertEqual(summary["candidate_set_snapshots"], 0)
        self.assertEqual(summary["verdict"], "observe_only_no_persistent_candidate_set")

    def test_loader_keeps_bad_lines_visible(self):
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", delete=False) as handle:
            handle.write(json.dumps(self._record(["BTCUSDT"])) + "\nnot-json\n")
            path = pathlib.Path(handle.name)
        records, invalid = MODULE.load_records(path)
        self.assertEqual(len(records), 1)
        self.assertEqual(invalid, 1)


if __name__ == "__main__":
    unittest.main()
