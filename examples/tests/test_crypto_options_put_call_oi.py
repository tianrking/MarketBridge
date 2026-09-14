import unittest

from crypto_options_put_call_oi_monitor import classify_ratio, summarize
from crypto_options_put_call_oi_replay import summarize_records
from crypto_options_put_call_oi_response_replay import aligned_observations, summarize as summarize_response


class OptionsPutCallOiTests(unittest.TestCase):
    def test_ratio_states_and_side_totals(self):
        rows = [
            {"payload": {"expiry_time": "2027-01-01T00:00:00Z", "option_type": "call", "open_interest": 100}},
            {"payload": {"expiry_time": "2027-01-01T00:00:00Z", "option_type": "put", "open_interest": 120}},
        ]
        result = summarize(rows, now_ts=1_735_689_600, max_expiry_days=1000)
        self.assertEqual(result["state"], "defensive_put_oi")
        self.assertEqual(result["call_oi"], 100)
        self.assertEqual(result["put_oi"], 120)
        self.assertAlmostEqual(result["put_call_oi_ratio"], 1.2)

    def test_missing_or_call_dominant_states_are_explicit(self):
        self.assertEqual(classify_ratio(None, 1.0, 0.6), "observe_only_missing_put_call_oi")
        self.assertEqual(classify_ratio(0.5, 1.0, 0.6), "call_dominant_oi")

    def test_persistence_and_response_replay_keep_state_labels(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"state": "defensive_put_oi"}},
            {"recorded_at_ms": 2, "observation": {"state": "defensive_put_oi"}},
            {"recorded_at_ms": 3, "observation": {"state": "balanced_oi"}},
        ]
        self.assertEqual(summarize_records(records, 2)["verdict"],
                         "persistent_defensive_put_oi_candidate")
        response_records = [
            {"recorded_at_ms": 1, "observation": {"oi": {"state": "defensive_put_oi"}, "price": {"price": 100}}},
            {"recorded_at_ms": 2, "observation": {"oi": {"state": "balanced_oi"}, "price": {"price": 101}}},
            {"recorded_at_ms": 3, "observation": {"oi": {"state": "call_dominant_oi"}, "price": {"price": 102}}},
        ]
        observations = aligned_observations(response_records, 1)
        self.assertEqual(len(observations), 2)
        self.assertEqual(summarize_response(observations, 1)["verdict"],
                         "options_put_call_oi_response_reported")


if __name__ == "__main__":
    unittest.main()
