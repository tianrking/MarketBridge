import importlib.util
import json
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1] / "crypto" / "microstructure"
SPEC = importlib.util.spec_from_file_location(
    "crypto_liquidity_stress_replay", ROOT / "crypto_liquidity_stress_replay.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class LiquidityStressReplayTests(unittest.TestCase):
    def _record(self, state="liquidity_stress", valid=True):
        metrics = {
            "spread_bps": 3.0,
            "buy_impact_bps": 8.0,
            "sell_impact_bps": 7.0,
        } if valid else {}
        return {
            "observation": {
                "state": state,
                "ewma_volatility_bps_per_bar": 30.0 if valid else None,
                "book": {"metrics": metrics},
            }
        }

    def test_load_records_counts_invalid_lines(self):
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", delete=False) as handle:
            handle.write(json.dumps(self._record()) + "\ninvalid\n")
            path = pathlib.Path(handle.name)
        records, invalid = MODULE.load_records(path)
        self.assertEqual(len(records), 1)
        self.assertEqual(invalid, 1)

    def test_persistent_stress_requires_consecutive_run(self):
        summary = MODULE.summarize_records([self._record(), self._record(), self._record()], 3)
        self.assertEqual(summary["longest_stress_run"], 3)
        self.assertEqual(summary["verdict"], "persistent_liquidity_stress_candidate")

    def test_missing_inputs_force_observe_only(self):
        summary = MODULE.summarize_records([self._record(valid=False)] * 3, 3)
        self.assertEqual(summary["valid_input_snapshots"], 0)
        self.assertEqual(summary["verdict"], "observe_only_no_persistent_stress")


if __name__ == "__main__":
    unittest.main()
