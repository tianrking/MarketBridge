import unittest

from crypto_options_max_pain_monitor import pain_proxy, summarize_expiry
from crypto_options_max_pain_replay import summarize_records
from crypto_options_max_pain_response_replay import aligned_observations, summarize as summarize_response


class OptionsMaxPainTests(unittest.TestCase):
    def test_pain_proxy_selects_strike_with_lowest_intrinsic_proxy(self):
        rows = [
            {"option_type": "call", "strike": 100, "open_interest": 10},
            {"option_type": "put", "strike": 100, "open_interest": 10},
        ]
        self.assertEqual(pain_proxy(100, rows), 0.0)
        self.assertGreater(pain_proxy(120, rows), 0.0)

    def test_near_expiry_state_keeps_distance_and_oi(self):
        rows = [
            {"payload": {"expiry_time": "2027-01-01T00:00:00Z", "option_type": "call",
                         "strike": 100, "open_interest": 10, "underlying_price": 101}},
            {"payload": {"expiry_time": "2027-01-01T00:00:00Z", "option_type": "put",
                         "strike": 100, "open_interest": 10, "underlying_price": 101}},
        ]
        result = summarize_expiry(rows, "2027-01-01T00:00:00Z", 1_798_761_600 - 2 * 86_400,
                                  3, 2, 0)
        self.assertEqual(result["max_pain_strike"], 100.0)
        self.assertAlmostEqual(result["open_interest"], 20.0)
        self.assertEqual(result["state"], "near_expiry_near_max_pain")

    def test_persistence_and_response_replay_preserve_states(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"target_expiry": {"state": "near_expiry_near_max_pain"}}},
            {"recorded_at_ms": 2, "observation": {"target_expiry": {"state": "near_expiry_near_max_pain"}}},
            {"recorded_at_ms": 3, "observation": {"target_expiry": {"state": "far_expiry_max_pain_context"}}},
        ]
        self.assertEqual(summarize_records(records, 2)["verdict"],
                         "persistent_near_expiry_max_pain_candidate")
        response_records = [
            {"recorded_at_ms": 1, "observation": {"max_pain": {"target_expiry": {"state": "near_expiry_near_max_pain"}}, "price": {"price": 100}}},
            {"recorded_at_ms": 2, "observation": {"max_pain": {"target_expiry": {"state": "near_expiry_far_from_max_pain"}}, "price": {"price": 101}}},
            {"recorded_at_ms": 3, "observation": {"max_pain": {"target_expiry": {"state": "far_expiry_max_pain_context"}}, "price": {"price": 102}}},
        ]
        observations = aligned_observations(response_records, 1)
        self.assertEqual(summarize_response(observations, 1)["verdict"], "max_pain_response_reported")


if __name__ == "__main__":
    unittest.main()
