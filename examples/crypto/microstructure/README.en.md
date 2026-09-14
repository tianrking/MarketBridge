# Crypto microstructure, squeeze and liquidation

> **Question:** do observable flow, depth, OI, liquidation, or volatility states
> differ in later price response without pretending to reveal trader intent?

## Case map

| Evidence | Entrypoints |
|---|---|
| Confluence monitors | `short_squeeze_monitor.py`, `exhaustion_short_monitor.py`, `liquidation_reversal_monitor.py`, `crypto_microstructure_monitor.py` |
| Flow and depth | `crypto_flow_book_confirmation.py`, `crypto_footprint_imbalance_*`, `crypto_spot_perp_depth_gap_*`, `crypto_liquidity_stress_*` |
| Two-sided walls | `crypto_liquidity_sandwich_monitor.py`, `crypto_liquidity_sandwich_response_recorder.py`, `crypto_liquidity_sandwich_response_replay.py` |
| Liquidation studies | `crypto_liquidation_burst_*`, `crypto_liquidation_price_cluster_*`, `liquidation_reversal_replay.py` |
| Event/technical replay | `crypto_cvd_divergence_replay.py`, `crypto_trade_imbalance_bar_replay.py`, `crypto_vpin_response_replay.py`, `crypto_*vwap*`, `crypto_*breakout*`, `crypto_session_*`, `crypto_weekday_hour_effect_replay.py` |
| Derivatives crowding | `crypto_taker_oi_response_replay.py`, `crypto_account_ratio_oi_response_replay.py`, `crypto_derivatives_*`, `crypto_adl_risk_*` |

The recorder/replay pairs freeze a state beside a quote and measure a later
fixed-record signed or absolute return. The ADL pair treats Binance's rating as
provider context—not proof that ADL occurred or a private account was at risk.
The liquidity-sandwich pair tests the narrower public-X claim that symmetric
near-touch bid and ask depth with a tight spread is followed by a different
absolute BTC move than ordinary snapshots. It does not call the displayed
levels persistent walls or infer a range-trading opportunity.

## Quickstart

```bash
python3 examples/crypto/microstructure/crypto_adl_risk_monitor.py \
  --symbol BTCUSDT --exchange binance
python3 examples/crypto/microstructure/crypto_adl_risk_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 60 \
  --output work/crypto-adl-risk-response.jsonl
python3 examples/crypto/microstructure/crypto_adl_risk_response_replay.py \
  --input work/crypto-adl-risk-response.jsonl --horizon-records 3 \
  --min-observations 5
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_monitor.py \
  --symbol BTCUSDT --exchange binance --depth-band-bps 10 \
  --min-side-depth-notional 100000 --min-symmetry-ratio 0.5
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --output work/crypto-liquidity-sandwich-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_replay.py \
  --input work/crypto-liquidity-sandwich-response.jsonl \
  --horizon-records 3 --min-observations 5
```

The full command set remains in [`README.md`](README.md). First polls may have
no OI baseline; venue liquidation side, trade side and book semantics are
provider-specific and must stay in the output.

## Evidence rules

- A confluence score is a screening observation, not an entry or exit signal.
- Missing OI, flow, liquidation or quote data is `observe_only`, never zero.
- A rolling buffer, candle approximation, or aggregate long/short ratio does
  not reveal ownership, intent, dealer sign, latent liquidation levels, or causality.
- Costs, funding, borrow, latency, slippage, queue position and fills are not
  silently inferred by these examples.

Provenance links and provider coverage notes are maintained in the source
catalog and include Binance's [ADL Risk API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk)
and public research leads such as this [liquidation discussion on X](https://x.com/angustias87/status/2039147109228925373).

## Boundary

This family never places orders, liquidates positions, signs wallets, or
interprets public aggregates as a user's private account state.
