# Bitcoin on-chain pressure

> **Question:** do public transfer, mempool, or mining snapshots provide a
> reproducible context for later BTC movement?

## Cases

- `crypto_onchain_transfer_burst_replay.py` and response recorder/replay:
  deduplicated public transfer rows and later absolute BTC movement.
- `crypto_onchain_mempool_pressure_monitor.py` / recorder / replay:
  recommended sat/vB and virtual-size pressure states.
- `crypto_onchain_mining_pressure_monitor.py` / recorder / replay:
  difficulty-adjustment and seven-day hashrate context.

## Quickstart

```bash
python3 examples/crypto/onchain/crypto_onchain_mempool_pressure_monitor.py \
  --high-fee-sat-vb 20 --low-fee-sat-vb 3 \
  --high-vsize-mb 150 --low-vsize-mb 25
python3 examples/crypto/onchain/crypto_onchain_mempool_pressure_recorder.py \
  --price-exchange binance --price-symbol BTCUSDT \
  --iterations 30 --interval-secs 60 \
  --output work/crypto-onchain-mempool-pressure.jsonl
python3 examples/crypto/onchain/crypto_onchain_mempool_pressure_replay.py \
  --input work/crypto-onchain-mempool-pressure.jsonl \
  --horizon-records 12 --min-observations 3
```

## Interpretation

Mempool fee guidance is not a confirmation-time guarantee. A transfer label is
not proof of exchange inflow/outflow, wallet ownership, or intent. Mining
difficulty and hashrate are network context, not a miner PnL or price forecast.
Provider/node snapshots, deduplication, timestamp alignment and missing BTC
quotes remain visible in every replay.

Provenance is maintained in [`README.md`](README.md), including the
[mempool.space REST API](https://mempool.space/docs/api/rest) and its
[fee guidance FAQ](https://mempool.space/docs/faq).

## Boundary

These examples do not inspect private wallets, broadcast transactions, choose
fees for a user, or execute transfers/mining operations.
