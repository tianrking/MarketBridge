"""Deterministic tests for paper funding-carry accounting."""

import unittest

from crypto_funding_carry_accrual_replay import paper_window, summarize


class FundingCarryAccrualReplayTests(unittest.TestCase):
    def test_short_perp_receives_positive_funding_and_basis_narrowing(self):
        funding = [(0, 0.001, 8), (10, 0.002, 8), (20, -0.001, 8)]
        spot = [(0, 100.0), (10, 100.0), (20, 100.0)]
        perp = [(0, 101.0), (10, 100.5), (20, 100.0)]
        row = paper_window(funding, spot, perp, 0, 2, "short_perp")
        self.assertAlmostEqual(row["gross_funding_pct"], 0.3)
        self.assertAlmostEqual(row["basis_pnl_pct"], 1.0)
        self.assertAlmostEqual(row["paper_carry_pct"], 1.3)

    def test_long_perp_reverses_the_decomposition(self):
        funding = [(0, 0.001, 8), (10, 0.002, 8), (20, -0.001, 8)]
        spot = [(0, 100.0), (10, 100.0), (20, 100.0)]
        perp = [(0, 101.0), (10, 100.5), (20, 100.0)]
        row = paper_window(funding, spot, perp, 0, 2, "long_perp")
        self.assertAlmostEqual(row["gross_funding_pct"], -0.3)
        self.assertAlmostEqual(row["basis_pnl_pct"], -1.0)
        self.assertAlmostEqual(row["paper_carry_pct"], -1.3)

    def test_summary_reports_funding_state_without_profitability_verdict(self):
        rows = [
            {"funding_state": "positive_funding", "paper_carry_pct": 0.5,
             "gross_funding_pct": 0.2, "basis_pnl_pct": 0.3},
            {"funding_state": "negative_funding", "paper_carry_pct": -0.5,
             "gross_funding_pct": -0.2, "basis_pnl_pct": -0.3},
        ]
        result = summarize(rows, 2)
        self.assertEqual(result["verdict"], "paper_carry_decomposition_reported")
        self.assertEqual(result["by_funding_state"]["positive_funding"]["windows"], 1)


if __name__ == "__main__":
    unittest.main()
