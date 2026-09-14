# Crypto research / 加密研究

> **Language / 语言:** [English](README.en.md) · [简体中文](README.zh-CN.md)

MarketBridge keeps strategy research in Python and the data/runtime plane in Rust.
The family guides below are the maintained source of truth for case maps,
commands, provenance and limitations.

| Family / 系列 | Evidence / 证据 | Guides / 指南 |
|---|---|---|
| Carry / 资金费率 | basis、funding、cross-venue | [`EN`](carry/README.en.md) · [`中文`](carry/README.zh-CN.md) |
| DeFi | pool、stablecoin、router impact | [`EN`](defi/README.en.md) · [`中文`](defi/README.zh-CN.md) |
| Macro / 宏观 | DXY、VIX、US10Y、ETF flow | [`EN`](macro/README.en.md) · [`中文`](macro/README.zh-CN.md) |
| Microstructure / 微结构 | flow、OI、depth、liquidation | [`EN`](microstructure/README.en.md) · [`中文`](microstructure/README.zh-CN.md) |
| On-chain / 链上 | transfer、mempool、mining | [`EN`](onchain/README.en.md) · [`中文`](onchain/README.zh-CN.md) |
| Options / 期权 | IV、skew、gamma、VRP | [`EN`](options/README.en.md) · [`中文`](options/README.zh-CN.md) |
| Sentiment / 情绪 | Fear & Greed、news、social | [`EN`](sentiment/README.en.md) · [`中文`](sentiment/README.zh-CN.md) |
| Universe / 资产宇宙 | breadth、ranking、pairs | [`EN`](universe/README.en.md) · [`中文`](universe/README.zh-CN.md) |

## Shared workflow / 统一流程

`monitor → recorder(JSONL) → replay → review coverage and invalidation`

每个案例都必须说明数据接口、provider、时间对齐、缺失值处理、前视规则、纸面成本和
研究限制。任何缺失值都不能静默变成零；任何 replay 都是描述性证据，不是自动化交易指令。

```bash
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance
```

新的案例必须放入对应系列，不要把 Python 文件直接堆在 `examples/crypto/` 根目录。

## Boundary / 边界

Rust 提供连接器、标准化、历史、缓存和 API；Python 负责假设、记录、回放、测试和文档。
示例不下单、不撤单、不签名钱包、不转移资金、不管理仓位，也不声称实盘 PnL。
