import importlib.util
import json
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1] / "crypto" / "options"
SPEC = importlib.util.spec_from_file_location(
    "crypto_options_vrp_replay", ROOT / "crypto_options_vrp_replay.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class OptionsVrpReplayTests(unittest.TestCase):
    def _record(self, value, expiry="2030-01-01", spot=100.0):
        return {
            "observation": {
                "target_expiry": {"expiry_time": expiry},
                "vrp": {"iv_minus_rv_iv_points": value},
                "realized_volatility": {"spot_close": spot},
            }
        }

    def test_load_records_preserves_invalid_lines(self):
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", delete=False) as handle:
            handle.write(json.dumps(self._record(6.0)) + "\nnot-json\n")
            path = pathlib.Path(handle.name)
        records, invalid = MODULE.load_records(path)
        self.assertEqual(len(records), 1)
        self.assertEqual(invalid, 1)

    def test_persistent_premium_regime(self):
        records = [self._record(6.0, spot=100.0), self._record(7.0, spot=101.0),
                   self._record(8.0, spot=102.0)]
        summary = MODULE.summarize_records(records, 5.0, 3)
        row = summary["by_expiry"]["2030-01-01"]
        self.assertEqual(row["longest_premium_run"], 3)
        self.assertEqual(row["verdict"], "persistent_vrp_candidate")

    def test_missing_vrp_is_observe_only(self):
        summary = MODULE.summarize_records([self._record(None)], 5.0, 1)
        row = summary["by_expiry"]["2030-01-01"]
        self.assertEqual(row["verdict"], "observe_only_no_persistent_vrp")


if __name__ == "__main__":
    unittest.main()
