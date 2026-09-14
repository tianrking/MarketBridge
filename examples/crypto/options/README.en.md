# Crypto options and volatility

> **Question:** do observable IV-surface states differ in later market response
> without inventing dealer ownership, hedge execution, or option PnL?

## Case map

- `crypto_options_skew_*` and term-structure replays: wing IV and near/far ATM
  slope persistence or BTC response.
- `crypto_options_panic_regime_response_replay.py`: a joint high-ATM-IV and
  positive put-minus-call-skew response study.
- `crypto_options_skew_vol_regime_response_replay.py`: a joint quiet-price-path
  and downside-skew response study.
- `crypto_options_put_call_oi_*`: provider open-interest composition as defensive,
  call-dominant, or balanced context.
- `crypto_options_gamma_*`, `crypto_options_max_pain_*`, and
  `crypto_options_bull_call_spread_*`: transparent surface/quote geometry studies.
- `crypto_options_vrp_*`, `crypto_deribit_*`, and historical-volatility replays:
  IV versus realized volatility and Deribit index responses.

## Quickstart

```bash
python3 examples/crypto/options/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30
python3 examples/crypto/options/crypto_options_skew_recorder.py \
  --currency BTC --venue deribit --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew.jsonl
python3 examples/crypto/options/crypto_options_skew_replay.py \
  --input work/crypto-options-skew.jsonl --min-run 3
```

Response pairs add a synchronized BTC quote and use `--horizon-records`; they
measure surface-state association, not option returns. Expiry identity, missing
greeks, moneyness buckets, quote freshness and coverage must remain visible.

The panic-regime replay keeps the MarketBridge convention explicit:
`put_call_skew_iv = put_iv - call_iv`, so positive values mean relatively higher
put-wing IV. It compares the joint state with one-dimensional and ordinary
states; a high-IV label is caller-supplied, not a universal market threshold.

```bash
python3 examples/crypto/options/crypto_options_panic_regime_response_replay.py \
  --input work/crypto-options-skew-response.jsonl \
  --horizon-records 3 --high-atm-iv 60 --downside-skew-iv 3 \
  --min-observations 5
```

The skew/volatility replay uses the same archive but computes a trailing,
unannualized standard deviation of log quote returns. It compares a
caller-defined low-volatility threshold plus positive put-minus-call skew with
one-dimensional and ordinary states. Because recorder cadence is configurable,
the threshold is a per-record proxy, not a 30-day or annualized volatility
claim.

```bash
python3 examples/crypto/options/crypto_options_skew_vol_regime_response_replay.py \
  --input work/crypto-options-skew-response.jsonl \
  --vol-window 6 --low-vol-pct 1.0 --downside-skew-iv 3 \
  --horizon-records 3 --min-observations 5
```

## Evidence rules

Gamma mass is an unsigned proxy unless a provider supplies an explicit sign.
Max pain is an intrinsic-distribution proxy, not settlement PnL or price pinning.
Put/call OI is not dealer sign or trader ownership. IV-RV spread is not a
short-volatility recommendation. No example models margin, delta hedging,
exercise, assignment, fill, fee, or execution.

See this guide for commands and provenance.

## Boundary

The options family is read-only: no options orders, hedges, wallet signatures,
position management, or live-account PnL claims.
