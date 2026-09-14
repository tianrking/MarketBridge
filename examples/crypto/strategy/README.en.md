# Python strategy runner

> **Purpose:** make the common read-only research cases easy to run from
> Python while keeping Rust as the MarketBridge data and API plane.

## Quick start

Start the local research service from the repository root:

```bash
MARKETBRIDGE_CONFIG=config.research.yaml cargo run
```

Then run one strategy in another terminal:

```bash
PYTHONPATH=examples/crypto/options:examples/crypto/carry:examples/crypto/microstructure:examples/crypto/universe \
  python3 examples/crypto/strategy/python_strategy_runner.py \
  --strategy funding_convergence --symbol BTCUSDT \
  --funding-exchanges binance,bybit,okx
```

List the available strategies and parameters with:

```bash
python3 examples/crypto/strategy/python_strategy_runner.py --help
```

## How to read the output

The JSON output keeps provider rows, timestamps, freshness/coverage fields and
an explicit `observe_only` result when the evidence is incomplete. The runner
does not fill missing observations with zero and does not convert a quote gap,
funding rate or aggregate OI value into a realized return.

For a deeper study, move from the runner to the family-specific monitor,
recorder and replay documented in [`../README.en.md`](../README.en.md). Those
scripts expose the hypothesis, horizon, sample threshold and paper-cost
assumptions more directly.

## Files

- `python_strategy_runner.py` — shared CLI and strategy dispatch.
- `strategy_entrypoint.py` — small compatibility wrapper for callers that need
  a stable launcher path.

## Boundary

This is a read-only research interface. It does not place, cancel or replace
orders, sign wallets, transfer funds, manage positions, or report live-account
P&L. Rust owns connectors, normalization, storage and APIs; Python owns the
hypothesis and interpretation layer.
