# Crypto carry and funding

> **Question:** when a basis, funding, or cross-venue relationship looks unusual,
> does the same observable state persist or show a measurable later response?

## What is here

| Group | Entrypoints | Evidence produced |
|---|---|---|
| Basis and funding | `basis_carry_monitor.py`, `funding_convergence_monitor.py`, `crypto_funding_band_monitor.py` | Fresh spot/perp basis, funding, interval, provider cap/floor and missing-field context |
| Historical replay | `crypto_basis_replay.py`, `crypto_historical_basis_replay.py`, `crypto_funding_*_replay.py` | Fixed-window contraction, convergence or regime distributions |
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

Use the full bilingual catalog in [`README.md`](README.md) for every command.
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

Research leads are listed in the source README and development log. The
provider-band semantics are grounded in Binance's [Funding Rate Info API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Get-Funding-Info);
public X posts are treated as hypotheses, never as validation. The latest
funding-band lead is [this public funding discussion](https://x.com/instaclaws/status/2038363051213181035).
The Coinbase premium lead is the [XWIN flow-confirmation discussion](https://x.com/xwinfinance/status/2023155692916646257).
Coinbase candle and market-data semantics are cross-checked against the
official [Coinbase Exchange candles API](https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles).

## Boundary

This family observes and replays public market data only. It never opens a
hedge, borrows inventory, routes transfers, signs a wallet, or sends an order.
