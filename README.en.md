# MarketBridge — English quickstart

> **Positioning:** read-only, multi-market data and strategy-research
> infrastructure. Rust is the data/runtime plane; strategies are Python-first.

MarketBridge collects public market data, normalizes it into stable APIs, and
supports bounded recording, replay, and paper validation. It is deliberately
not a trading bot: it never signs wallets, sends orders, moves funds, manages
positions, or claims live-account PnL.

## Choose the right entry point

| You want to… | Start here |
|---|---|
| Run the local data service | [`config.research.yaml`](config.research.yaml) and the [quick start](#quick-start) below |
| Browse Python strategy cases | [`examples/README.en.md`](examples/README.en.md) |
| Run the shared Python strategy CLI | [`examples/crypto/strategy/README.en.md`](examples/crypto/strategy/README.en.md) |
| Read the crypto family map | [`examples/crypto/README.en.md`](examples/crypto/README.en.md) |
| Check API contracts and field semantics | [`docs/data_interfaces.md`](docs/data_interfaces.md) |
| Understand implemented versus planned features | [`docs/feature_inventory.md`](docs/feature_inventory.md) |
| Review research acceptance rules | [`docs/user-guide/12-strategy-intake.md`](docs/user-guide/12-strategy-intake.md) |
| Read the full architecture/API reference | [`README.md`](README.md) |

## Quick start

From the repository root:

```bash
# Start a localhost-only, read-only research service.
MARKETBRIDGE_CONFIG=config.research.yaml cargo run

# In another terminal, open the workbench and supervise the Python scanner.
python3 scripts/start_marketbridge_analysis.py \
  --base-url http://127.0.0.1:8080 --limit 100 \
  --minimum-score 5 --candle-workers 8 --interval-secs 30

# Or inspect one Python case directly.
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance
```

The supervisor waits for the Rust API, opens `http://127.0.0.1:8080/workbench`,
and keeps the read-only Binance research scanner alive. Use `--no-browser` on
a headless host. The workbench exposes observed facts, freshness, missing
fields, research states, and auditable reference levels; it never places an
order or signs a wallet.

The monitor prints the normalized payload, freshness, provider coverage, and
an explicit `observe_only` result when required evidence is missing. To create
a reproducible sample, run a `*_recorder.py` into `work/*.jsonl`, then pass the
archive to the matching `*_replay.py` with an explicit horizon and minimum
sample size.

## How the layers fit together

```text
public connectors → Rust normalization/cache/history/API → Python monitor
                                                    ↘ recorder → replay
```

- Rust owns connectors, symbol mapping, freshness/coverage metadata, history,
  REST/WebSocket APIs, and runtime health.
- Python owns hypothesis-specific monitors, recorders, replays, tests, and
  bilingual research documentation.
- A case is accepted only when its provider, timestamp alignment, look-ahead
  rule, missing-data behavior, paper-cost assumption, and invalidation limits
  are visible in code and output.

## Research boundary

Examples are evidence tools, not execution adapters. A quote gap is not a fill;
a funding rate is not funding income; a public aggregate is not a private
account state; and a small or incomplete sample is not proof of a strategy.
Borrow, fees, funding, transfers, latency, slippage, queue position, margin,
and venue solvency remain explicit research gaps unless a case documents a
paper assumption. Missing data stays missing and is never silently replaced by
zero.

## Verification

```bash
PYTHONPATH=examples/crypto/options:examples/crypto/defi:examples/crypto/microstructure:examples/crypto/onchain:examples/crypto/carry:examples/crypto/macro:examples/crypto/universe:examples/crypto/sentiment:examples/crypto/strategy:examples/prediction:examples/weather \
  python3 -m unittest discover -s examples/tests -p 'test_*.py'
python3 -m compileall -q examples
rustup run 1.97.1 cargo test --locked
rustup run 1.97.1 cargo clippy --locked --all-targets --all-features -- -D warnings
```

## Further reading

- [Full English architecture and API reference](README.md)
- [简体中文项目说明](README.zh-CN.md)
- [English examples guide](examples/README.en.md) · [中文案例 guide](examples/README.zh-CN.md)
- [Development evidence and provenance](docs/development-log.md)
- [Data-interface contracts](docs/data_interfaces.md)

The documentation is intentionally split into three layers: this quickstart
for orientation, the bilingual `examples/` guides for runnable research cases,
and `docs/` for API contracts, operations and architecture. Each layer keeps
the same read-only boundary and links to the next level of detail.
