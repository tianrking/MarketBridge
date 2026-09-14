# Bitcoin 链上压力研究

> **研究问题：** 公开转账、mempool 或矿工快照，能否构成可复现的 BTC 后续波动上下文？

## 案例

- `crypto_onchain_transfer_burst_replay.py` 及 response recorder/replay：去重公开转账行，
  比较后续 BTC 绝对波动。
- `crypto_onchain_mempool_pressure_monitor.py` / recorder / replay：推荐 sat/vB 和虚拟大小压力状态。
- `crypto_onchain_mining_pressure_monitor.py` / recorder / replay：难度调整和七日 hashrate 上下文。

## 快速开始

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

## 解释

mempool 手续费建议不是确认时间保证。转账标签不等于交易所净流入/流出、钱包归属或交易意图。
难度和 hashrate 是网络上下文，不是矿工 PnL 或价格预测。提供方/节点快照、去重、时间对齐和 BTC 报价
缺失都会在回放中保留。

出处在 [`README.md`](README.md) 维护，包括 [mempool.space REST API](https://mempool.space/docs/api/rest)
和[手续费建议 FAQ](https://mempool.space/docs/faq)。

## 边界

示例不读取私人钱包、不广播交易、不替用户选择手续费，也不执行转账或挖矿操作。
