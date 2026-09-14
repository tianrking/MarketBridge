# MarketBridge examples

> **Scope:** Python-first, read-only market research. Rust remains the data
> plane; these examples request data, record observations, replay hypotheses,
> and report evidence.

This is the short English entry point for the demo library. The full script
catalogue is kept in [`README.md`](README.md); each family below has a matching
Chinese guide at `README.zh-CN.md`.

## Choose a family

| Family | Use it for | Guide |
|---|---|---|
| Crypto carry | basis, funding, cross-venue and triangular price relationships | [`crypto/carry/`](crypto/carry/README.en.md) |
| Crypto DeFi | pool flow, stablecoins, router impact and liquidity context | [`crypto/defi/`](crypto/defi/README.en.md) |
| Crypto macro | DXY/VIX/US10Y, ETF flow, stablecoin liquidity impulse and market regime context | [`crypto/macro/`](crypto/macro/README.en.md) |
| Crypto microstructure | order flow, OI, liquidation, depth and response studies | [`crypto/microstructure/`](crypto/microstructure/README.en.md) |
| Crypto on-chain | transfers, mempool and mining pressure | [`crypto/onchain/`](crypto/onchain/README.en.md) |
| Crypto options | IV surface, skew, gamma, VRP and max-pain proxies | [`crypto/options/`](crypto/options/README.en.md) |
| Crypto sentiment | fear/greed, news attention and social metrics | [`crypto/sentiment/`](crypto/sentiment/README.en.md) |
| Crypto universe | breadth, cross-asset ranking and relative value | [`crypto/universe/`](crypto/universe/README.en.md) |
| Prediction markets | public trade flow, calibration and settlement replay | [`prediction/`](prediction/README.en.md) |
| Weather | deterministic observations and market-calibration inputs | [`weather/`](weather/README.en.md) |

## Standard research loop

1. Start MarketBridge with a read-only research configuration.
2. Run a `*_monitor.py` once to inspect the live payload and missing fields.
3. Run a `*_recorder.py` to create an append-only JSONL observation archive.
4. Run a `*_replay.py` with an explicit horizon, minimum sample, and paper cost.
5. Treat `observe_only`, incomplete coverage, provider semantics, and small
   samples as results—not as zeros to be filled in.

```bash
MARKETBRIDGE_CONFIG=config.research.yaml cargo run
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance
```

Most recorders default to `work/*.jsonl`. The files are research artifacts;
they do not contain private keys, signed transactions, or order instructions.

## Non-negotiable boundary

MarketBridge examples never place, cancel, or replace orders; sign wallets;
move funds; manage positions; or claim live-account PnL. A strategy example is
accepted only when its input provider, timestamp, coverage, look-ahead rule,
cost assumption, and invalidation/limitation are visible in code and output.

## Verification

From the repository root:

```bash
PYTHONPATH=examples/crypto/options:examples/crypto/defi:examples/crypto/microstructure:examples/crypto/onchain:examples/crypto/carry:examples/crypto/macro:examples/crypto/universe:examples/crypto/sentiment:examples/crypto/strategy:examples/prediction:examples/weather \
  python3 -m unittest discover -s examples/tests -p 'test_*.py'
python3 -m compileall -q examples
```

See [`docs/user-guide/12-strategy-intake.md`](../docs/user-guide/12-strategy-intake.md)
for the acceptance checklist and [`README.md`](README.md) for the complete
case-by-case inventory and provenance links.
