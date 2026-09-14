# DeFi flow and liquidity / DeFi 流量与流动性

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

覆盖池状态、swap flow、稳定币供给、router impact、DLMM/Whirlpool/AMM 参数和收益上下文。
这些字段用于研究流动性压力与后续响应，不代表 LP 收益、可成交深度或链上意图。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Pool flow | `crypto_defi_pool_flow_*` |
| Stablecoins | `crypto_stablecoin_*` |
| Router/pool venues | `crypto_jupiter_*`, `crypto_meteora_*`, `crypto_orca_*`, `crypto_raydium_*` |
| Yield context | `crypto_defi_funding_yield_*`, `crypto_defi_yield_context_monitor.py` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/defi/crypto_defi_pool_flow_monitor.py \
  --symbol SOLUSDC --min-turnover-ratio 1.0
python3 examples/crypto/defi/crypto_defi_pool_flow_recorder.py \
  --symbol SOLUSDC --iterations 30 --interval-secs 60 \
  --output work/crypto-defi-pool-flow.jsonl
python3 examples/crypto/defi/crypto_defi_pool_flow_replay.py \
  --input work/crypto-defi-pool-flow.jsonl --min-run 3
```

完整参数、历史稳定币回放、provider provenance 和中文解释见双语指南。

## Boundary / 边界

不会连接钱包、签名交易、构造 swap、管理 LP 头寸或承诺成交。部分上游、过期快照、缺失 mint
和 provider 模型变化必须原样标记为 coverage gap，而不是零流动性。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/defi/<entrypoint>.py`；参数、出处和限制见上述双语指南。
