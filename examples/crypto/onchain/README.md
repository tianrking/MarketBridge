# On-chain pressure / 链上转账与网络压力

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

覆盖公开转账、Bitcoin mempool 费率/虚拟大小、难度调整、hashrate、Hash Ribbon 和 Mayer
Multiple。案例测试网络上下文与 BTC 响应，不读取私人钱包，也不推断地址意图。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Transfers | `crypto_onchain_transfer_burst_*` |
| Mempool | `crypto_onchain_mempool_pressure_*` |
| Mining | `crypto_onchain_mining_pressure_*`, `crypto_hash_ribbon_*` |
| Valuation context | `crypto_mayer_multiple_response_replay.py` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/onchain/crypto_onchain_mempool_pressure_monitor.py \
  --high-fee-sat-vb 20 --low-fee-sat-vb 3 \
  --high-vsize-mb 150 --low-vsize-mb 25
```

完整 recorder/replay 命令和 mempool.space provenance 见双语指南。

## Boundary / 边界

推荐费率不是确认时间保证，转账标签不是交易所流入/流出证明，hashrate 不是矿工 PnL。
不会广播交易、选择用户手续费或执行任何链上操作。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/onchain/<entrypoint>.py`；参数、出处和限制见上述双语指南。
