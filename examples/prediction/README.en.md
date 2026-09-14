# Prediction-market research

> Public-market, book, trade and settlement observations for calibration and
> timing research. No orders and no wallet signatures.

## Cases

- `polymarket_complement_monitor.py`: YES/NO complement-price snapshot.
- `prediction_trade_flow.py`, `polymarket_trade_recorder.py`: bounded public
  trade-flow archive.
- `polymarket_price_shock_replay.py`, `polymarket_timing_replay.py`: descriptive
  timing and continuation studies.
- `polymarket_settlement_replay.py`, `polymarket_calibration_report.py`:
  resolved-outcome scoring, calibration bins, Brier and log loss.

Use the complete command examples in [`../README.md`](../README.md). Every
replay must state market identity, resolution rule, observed start time, missing
trades, price/fee assumptions and the fact that public trades are not a private
fill ledger.

## Boundary

These examples do not submit, cancel, or sign prediction-market transactions;
they only consume public data and produce bounded paper evidence.
