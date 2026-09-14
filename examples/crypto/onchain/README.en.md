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
- `crypto_hash_ribbon_response_replay.py`: a timestamp-aware 30/60-day
  hashrate-crossing response study using `/v1/history/mining` and BTC candles.
- `crypto_mayer_multiple_response_replay.py`: a close-only 200-day price/SMA
  regime study across discount, trend-band and premium states.

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

The hash-ribbon case follows the public 30/60-day moving-average definition
described by [Glassnode](https://studio.glassnode.com/charts/indicators.HashRibbon)
and tests it as a response distribution only. It requires timestamp coverage
for the requested windows, keeps the optional 10/20-day price-momentum
confirmation separate, and never labels a provider estimate as miner
capitulation or a buy signal.

```bash
python3 examples/crypto/onchain/crypto_hash_ribbon_response_replay.py \
  --mining-window 3y --short-window-days 30 --long-window-days 60 \
  --price-short-days 10 --price-long-days 20 \
  --horizon-days 30 --min-observations 3
python3 examples/crypto/onchain/crypto_mayer_multiple_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1d --days 1825 \
  --ma-days 200 --discount-threshold 0.8 \
  --deep-discount-threshold 0.6 --premium-threshold 2.4 \
  --extreme-premium-threshold 3.0 --horizon-days 30 \
  --min-observations 5
```

Provenance is maintained in [`README.md`](README.md), including the
[mempool.space REST API](https://mempool.space/docs/api/rest) and its
[fee guidance FAQ](https://mempool.space/docs/faq).

## Boundary

These examples do not inspect private wallets, broadcast transactions, choose
fees for a user, or execute transfers/mining operations.
