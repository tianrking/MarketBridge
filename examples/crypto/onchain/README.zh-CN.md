# Bitcoin 链上压力研究

> **研究问题：** 公开转账、mempool 或矿工快照，能否构成可复现的 BTC 后续波动上下文？

## 案例

- `crypto_onchain_transfer_burst_replay.py` 及 response recorder/replay：去重公开转账行，
  比较后续 BTC 绝对波动。
- `crypto_onchain_mempool_pressure_monitor.py` / recorder / replay：推荐 sat/vB 和虚拟大小压力状态。
- `crypto_onchain_mining_pressure_monitor.py` / recorder / replay：难度调整和七日 hashrate 上下文。
- `crypto_hash_ribbon_response_replay.py`：使用 `/v1/history/mining` 与 BTC K 线，做时间戳感知的
  30/60 日 hashrate 交叉响应研究。
- `crypto_mayer_multiple_response_replay.py`：使用收盘价与 200 日 SMA，比较折价、趋势带和溢价状态的
  后续 BTC 响应。

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

Hash Ribbon 案例参考 [Glassnode 的 30/60 日定义](https://studio.glassnode.com/charts/indicators.HashRibbon)，
但只把交叉当作响应分布标签。窗口必须有时间覆盖，10/20 日价格动量确认保持为独立过滤条件；不会把提供方估计称为
矿工投降，也不会输出买入信号。

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

出处在本中文指南维护，包括 [mempool.space REST API](https://mempool.space/docs/api/rest)
和[手续费建议 FAQ](https://mempool.space/docs/faq)。

## 边界

示例不读取私人钱包、不广播交易、不替用户选择手续费，也不执行转账或挖矿操作。
