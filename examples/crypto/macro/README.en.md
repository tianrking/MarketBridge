# Crypto macro context

> **Question:** do public macro references, ETF flow, or aggregate regime labels
> separate later crypto responses after timestamp alignment?

## Cases

- `crypto_macro_context_monitor.py` / recorder / replay: configured DXY, VIX,
  US10Y and perp funding context joined with a BTC quote.
- `crypto_etf_flow_response_recorder.py` / replay: caller-supplied or
  `farside_etf` flow snapshots aligned to daily BTC candles. The replay also
  supports a trailing flow window so persistence can be tested separately from
  a single-day threshold.
- `crypto_market_regime_monitor.py` / recorder / replay: Rust aggregate regime
  labels and fixed-window BTC response distributions.

## Quickstart

```bash
python3 examples/crypto/macro/crypto_macro_context_monitor.py \
  --symbol BTCUSDT --exchange binance --vix-risk-threshold 25 \
  --funding-extreme-pct 0.01
python3 examples/crypto/macro/crypto_macro_context_recorder.py \
  --symbol BTCUSDT --exchange binance --product-type perp \
  --iterations 30 --interval-secs 30 \
  --output work/crypto-macro-context.jsonl
python3 examples/crypto/macro/crypto_macro_context_replay.py \
  --input work/crypto-macro-context.jsonl --horizon-records 7 \
  --min-observations 5 --paper-cost-bps 10
```

ETF flow is intentionally an explicit data boundary. A Farside-style CSV or
configured connector may be used, but a blocked page is a fetch failure—not a
zero flow. Provider timestamps are snapshots and do not create a synchronized
historical index series.

For a trailing-window study, pass `--rolling-observations 5`. The replay uses
only the current and prior external rows, skips incomplete windows, and labels
`rolling_inflow`, `rolling_outflow`, or `rolling_neutral`. The default rolling
threshold is the daily threshold multiplied by the window length; override it
with `--rolling-threshold-musd` when the research question calls for a
cumulative USD-million hurdle.

The persistence lead is motivated by this public [ecoinometrics ETF-flow
discussion](https://x.com/ecoinometrics/status/2037548621697303004). It is an
unverified research lead, not evidence that a rolling threshold predicts BTC.

## Interpretation and boundary

Macro context is conditioning information, not a return forecast or allocation
decision. The examples do not trade ETFs, rebalance a portfolio, or claim that
correlation is causal. Keep calendar alignment, publication delay, missing
observations, sample size and paper costs in every report.

See [`README.md`](README.md) for the CSV schema, commands and provenance links.
