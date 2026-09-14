import unittest

from crypto_account_ratio_oi_response_replay import classify_state, summarize


class AccountRatioOiResponseTests(unittest.TestCase):
    def test_crowding_and_unwinding_states_are_separate(self):
        self.assertEqual(classify_state(0.2, 0.2, 0.1, 0.1), "long_holder_crowding_oi_rising")
        self.assertEqual(classify_state(0.2, -0.2, 0.1, 0.1), "long_holder_unwinding_oi_falling")
        self.assertEqual(classify_state(-0.2, 0.2, 0.1, 0.1), "short_holder_crowding_oi_rising")

    def test_missing_alignment_is_not_ordinary(self):
        self.assertEqual(classify_state(0.2, None, 0.1, 0.1), "observe_only_missing_alignment")

    def test_summary_keeps_signed_states_separate(self):
        result = summarize([
            {"state": "long_holder_crowding_oi_rising", "forward_return_pct": 1.0},
            {"state": "long_holder_unwinding_oi_falling", "forward_return_pct": -1.0},
        ])
        self.assertEqual(result["long_holder_crowding_oi_rising"]["observations"], 1)
        self.assertEqual(result["long_holder_unwinding_oi_falling"]["mean_forward_return_pct"], -1.0)


if __name__ == "__main__":
    unittest.main()
