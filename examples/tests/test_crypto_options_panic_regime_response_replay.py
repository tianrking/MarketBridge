"""Deterministic tests for joint option-IV panic regime replay."""

import tempfile
import unittest
from pathlib import Path

from crypto_options_panic_regime_response_replay import (
    load_records,
    panic_state,
    summarize_records,
)


class OptionsPanicRegimeResponseReplayTests(unittest.TestCase):
    def test_marketbridge_skew_convention_is_explicit(self):
        self.assertEqual(panic_state({"atm_iv": 65, "put_call_skew_iv": 5}, 60, 3),
                         "high_iv_downside_skew")
        self.assertEqual(panic_state({"atm_iv": 65, "put_call_skew_iv": -5}, 60, 3),
                         "high_iv_without_downside_skew")

    def test_jsonl_replay_compares_joint_state(self):
        records = []
        for index, price in enumerate([100.0, 99.0, 95.0, 96.0]):
            records.append({
                "recorded_at_ms": index,
                "observation": {
                    "price": {"price": price},
                    "target_expiry": {"atm_iv": 65.0, "put_call_skew_iv": 5.0},
                },
            })
        result = summarize_records(records, 1, 60.0, 3.0, 1)
        self.assertEqual(result["high_iv_downside_skew_windows"], 3)
        self.assertEqual(result["verdict"], "options_panic_regime_response_reported")

    def test_invalid_jsonl_is_counted(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "records.jsonl"
            path.write_text("not-json\n{}\n", encoding="utf-8")
            records, invalid = load_records(path)
        self.assertEqual(records, [])
        self.assertEqual(invalid, 2)


if __name__ == "__main__":
    unittest.main()
