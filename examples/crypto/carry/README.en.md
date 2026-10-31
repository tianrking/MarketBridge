# Crypto carry and funding

> **Question:** when a basis, funding, or cross-venue relationship looks unusual,
> does the same observable state persist or show a measurable later response?

## What is here

| Group | Entrypoints | Evidence produced |
|---|---|---|
| Basis and funding | `basis_carry_monitor.py`, `funding_convergence_monitor.py`, `crypto_funding_band_monitor.py` | Fresh spot/perp basis, funding, interval, provider cap/floor and missing-field context |
| Historical replay | `crypto_basis_replay.py`, `crypto_historical_basis_replay.py`, `crypto_funding_*_replay.py` | Fixed-window contraction, convergence or regime distributions |
| Cross-sectional funding | `crypto_funding_cross_section_replay.py` | Compare raw funding ranks and per-contract historical z-score ranks against fixed-window forward returns |
| Cross-venue | `crypto_cross_venue_orderbook_*`, `crypto_cross_venue_price_gap_replay.py` | Point-in-time quote/book gaps with latency and coverage metadata |
| Coinbase premium | `crypto_coinbase_premium_monitor.py`, `crypto_coinbase_premium_response_recorder.py`, `crypto_coinbase_premium_response_replay.py` | Coinbase USD versus reference-venue spot spread and later BTC response |
| Triangular | `crypto_triangular_arbitrage_*` | Paper price-cycle consistency only; no route or fill |
| Response studies | `crypto_*_response_recorder.py` / `*_response_replay.py` | State frozen beside a BTC quote, then compared with later returns |

The funding-band pair is the newest response case. It classifies provider
funding as `near_upper_funding_cap`, `near_lower_funding_floor`, or
`within_provider_funding_band`, then compares later BTC movement. It does not
turn a cap/floor proximity observation into funding income, a hedge, or a trade.

The Coinbase premium pair tests a separate public-X lead: after the Coinbase
USD quote is materially above or below a reference spot quote, does the next
fixed-record BTC response differ from ordinary snapshots? The default
reference is Binance `BTCUSDT`; the USD/USDT stablecoin basis is retained as a
limitation rather than silently labelled US spot flow.
`crypto_coinbase_premium_historical_replay.py` runs the same hypothesis over
bounded Coinbase and reference-venue candles served by
`/v1/history/candles`, so the live recorder is not the only route to evidence.

Historical OI now accepts Binance, Bybit, and OKX. OKX's public contract
history is an aggregate base-currency series returned in provider USD units;
the API keeps that unit and its timestamp explicit. This widens the
`crypto_funding_oi_replay.py` venue comparison without turning aggregate OI
into long/short ownership or a hedge result.

`crypto_funding_cross_section_replay.py` compares `--rank-by raw` (the default)
with `--rank-by zscore`. The z-score compares the current rate with the prior
`--zscore-lookback` settlements for the same contract; it does not use
the current or future rates to construct its baseline. The default of 270 events
approximates 90 calendar days only for an 8-hour settlement schedule; the option
is event-count based, not a precise time window. Contracts without enough
history or with a constant lookback are omitted. Raw funding dispersion still
filters eligible timestamps. This replay uses only the selected symbols and
bounded history returned by the API; it does not reproduce a broad,
delisting-aware research universe.

The public [funding-rate-alpha research repository](https://github.com/OctopusTakopi/funding-rate-alpha)
publishes code, methods and source manifests testing an X-origin funding-factor
claim. Its reported directional relationship changes across periods and
holding horizons. OI/price interactions and an OI-weighted cross-venue funding
index do not show a stable incremental directional edge; the authors attribute
the remaining results mainly to carry while holding perpetual price risk. This
is a reproducible research note, not a peer-reviewed conclusion. The repository
does not include the full raw data; reproducing its full result requires the
Binance, OKX and Bybit archives and delisted-contract universe.

Evidence mapping: **directly verifiable** — compare raw and strictly
prior-history z-score rankings against later prices for selected contracts;
**needs broader data** — reproduce the 791-contract, delisted-inclusive sample
and cross-venue funding/OI controls; **X post is a hypothesis** — the
repository's English rendering is in [`source_tweet.md`](https://github.com/OctopusTakopi/funding-rate-alpha/blob/main/docs/source_tweet.md),
but the original X post could not be independently read in this run. The
available evidence does not establish a robust, general short rule.

```bash
python3 examples/crypto/carry/crypto_funding_cross_section_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --funding-exchange binance \
  --price-exchange binance --market perp --interval 4h --days 90 \
  --horizon-bars 2 --rank-by zscore --zscore-lookback 270 \
  --min-observations 5
```

`cross_sectional_funding_candidate` only means that results exceed thresholds
chosen on the command line; it is not an out-of-sample, cost or multiple-testing
pass. Check per-symbol JSON coverage before widening the universe. The replay
does not add OI/price interaction controls, trade-level execution costs,
leverage or liquidation modelling.

`crypto_funding_carry_accrual_replay.py` is the accounting companion to the
state replays. For each fixed number of funding events it computes gross
funding transfer per unit notional, the observed spot/perpetual basis change,
and their signed paper sum for `short_perp` or `long_perp`. It is deliberately
not an executable carry backtest: public closes are not fills, and borrow,
margin, collateral, fees and slippage remain outside the ledger.

`crypto_historical_basis_replay.py` tests a separate provider-basis hypothesis:
after an unusually wide Binance basis-rate observation, does the absolute basis
contract over the next fixed provider window more often than ordinary states?
Use `--basis-pages` (1–48) when the requested period needs more than one bounded
500-row provider page. The API reports the requested page count and covered
timestamps; Binance's public retention remains bounded, so this is not a full
history or executable carry PnL.

```bash
python3 examples/crypto/carry/crypto_historical_basis_replay.py \
  --symbol BTCUSDT --period 1h --days 20 --limit 500 --basis-pages 2 \
  --horizon-bars 3 --extreme-threshold-bps 50 --min-observations 5
```

```bash
python3 examples/crypto/carry/crypto_funding_carry_accrual_replay.py \
  --symbol BTCUSDT --funding-exchange binance \
  --spot-exchange binance --perp-exchange binance \
  --price-interval 5m --days 7 --funding-pages 2 --horizon-events 3 \
  --position-side short_perp --min-observations 5
```

Funding-rate candles on Binance, Bybit and OKX also accept bounded pagination;
the carry ledger exposes it as `--funding-pages`. Provider settlement intervals
and coverage remain explicit.

`crypto_funding_interval_change_response_replay.py` uses adjacent provider
funding timestamps to test whether a changed settlement interval has a different
later BTC response from stable-interval observations. It does not assume every
venue settles every eight hours or convert the observed rate into annualized
income.

```bash
python3 examples/crypto/carry/crypto_funding_interval_change_response_replay.py \
  --exchange okx --symbol BTCUSDT --period 8h --days 30 \
  --funding-pages 4 --price-pages 4 --horizon-bars 3 --min-observations 5
```

## Run a complete case

```bash
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance --threshold 0.8
python3 examples/crypto/carry/crypto_funding_band_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 600 \
  --output work/crypto-funding-band-response.jsonl
python3 examples/crypto/carry/crypto_funding_band_response_replay.py \
  --input work/crypto-funding-band-response.jsonl \
  --horizon-records 3 --min-observations 5
```

Use this guide for the maintained command examples, provenance and limitations.
Recorders append JSONL; remove an old file intentionally before starting a new
sample so that independent studies do not share state.

## Interpretation rules

- A funding interval or provider band that is missing is evidence of missing
  metadata, not a zero interval or an inferred annualized yield.
- A replay horizon is a number of recorded observations, not automatically a
  number of hours; inspect timestamps before comparing studies.
- Basis and cross-venue edges are gross observations until borrow, fees,
  transfer latency, inventory, slippage, queue position and fill coverage are
  modelled separately.
- `research_only_no_orders` is part of every recorder/replay result.

```bash
python3 examples/crypto/carry/crypto_coinbase_premium_monitor.py \
  --coinbase-symbol BTC-USD --reference-symbol BTCUSDT \
  --reference-exchange binance --premium-threshold-bps 5
python3 examples/crypto/carry/crypto_coinbase_premium_response_recorder.py \
  --coinbase-symbol BTC-USD --reference-symbol BTCUSDT \
  --reference-exchange binance --iterations 60 --interval-secs 60 \
  --output work/crypto-coinbase-premium-response.jsonl
python3 examples/crypto/carry/crypto_coinbase_premium_response_replay.py \
  --input work/crypto-coinbase-premium-response.jsonl \
  --horizon-records 3 --min-observations 5
python3 examples/crypto/carry/crypto_coinbase_premium_historical_replay.py \
  --coinbase-symbol BTCUSDT --reference-symbol BTCUSDT \
  --reference-exchange binance --interval 1h --days 14 \
  --horizon-bars 3 --min-observations 5
```

## Provenance

Research leads are listed in this guide and the development log. The
provider-band semantics are grounded in Binance's [Funding Rate Info API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Get-Funding-Info);
public X posts are treated as hypotheses, never as validation. The latest
funding-band lead is [this public funding discussion](https://x.com/instaclaws/status/2038363051213181035).
The paper-carry accounting is cross-checked against Binance's [funding-arbitrage explanation](https://www.binance.com/en/support/faq/detail/61012e690cf343e7979649282a2ccc3c)
and Kraken's [funding-rate strategy overview](https://www.kraken.com/learn/futures-trading-funding-rate-strategy); both motivate a cash-flow decomposition, not a guaranteed return.
The Coinbase premium lead is the [XWIN flow-confirmation discussion](https://x.com/xwinfinance/status/2023155692916646257).
Coinbase candle and market-data semantics are cross-checked against the
official [Coinbase Exchange candles API](https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles).

## Boundary

This family observes and replays public market data only. It never opens a
hedge, borrows inventory, routes transfers, signs a wallet, or sends an order.
