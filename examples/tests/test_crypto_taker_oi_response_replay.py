import unittest

from crypto_taker_oi_response_replay import (
    classify_state,
    forward_return,
    oi_change_at,
    summarize,
)


class TakerOiResponseTests(unittest.TestCase):
    def test_new_buy_pressure_and_absorption_are_separate(self):
        self.assertEqual(classify_state(0.4, 0.2, 0.2, 0.1), "buy_pressure_oi_rising")
        self.assertEqual(classify_state(0.4, -0.2, 0.2, 0.1), "buy_absorption_oi_falling")
        self.assertEqual(classify_state(-0.4, 0.2, 0.2, 0.1), "sell_pressure_oi_rising")

    def test_oi_change_and_forward_return_are_point_in_time(self):
        oi = [(1, 100.0), (2, 110.0), (3, 105.0)]
        prices = [(2, 100.0), (3, 101.0), (4, 99.0), (5, 100.0)]
        self.assertAlmostEqual(oi_change_at(3, oi), -4.5454545, places=5)
        self.assertAlmostEqual(forward_return(2, prices, 2), -1.0)

    def test_missing_alignment_stays_observe_only(self):
        self.assertEqual(classify_state(0.4, None, 0.2, 0.1), "observe_only_missing_alignment")

    def test_summary_keeps_states_separate(self):
        result = summarize([
            {"state": "buy_pressure_oi_rising", "forward_return_pct": 1.0},
            {"state": "buy_absorption_oi_falling", "forward_return_pct": -1.0},
        ])
        self.assertEqual(result["buy_pressure_oi_rising"]["observations"], 1)
        self.assertEqual(result["buy_absorption_oi_falling"]["mean_absolute_forward_return_pct"], 1.0)


if __name__ == "__main__":
    unittest.main()
