# Crypto macro context / 加密宏观上下文

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

用 DXY、VIX、US10Y、ETF 流量、稳定币供给和聚合市场状态构造时间对齐的上下文，检验其与
BTC 后续响应的关系。外部 CSV/JSONL 是调用者输入，阻断或缺失不能填成零。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Macro context | `crypto_macro_context_*` |
| ETF/liquidity | `crypto_etf_flow_*`, `crypto_liquidity_*` |
| Market regime | `crypto_market_regime_*` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/macro/crypto_macro_context_monitor.py \
  --symbol BTCUSDT --exchange binance --vix-risk-threshold 25 \
  --funding-extreme-pct 0.01
python3 examples/crypto/macro/crypto_macro_context_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 30 \
  --output work/crypto-macro-context.jsonl
```

历史 ETF/稳定币研究使用双语指南中列出的 `*_replay.py`，并明确外部来源、日期对齐和
`--candle-pages` 的有界历史范围。

## Boundary / 边界

宏观状态不是价格预测，也不是资金流入证明；不构造组合、不分配资金、不下单。Rust API 的
时间戳、provider coverage 和历史分页必须在结果中可见。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/macro/<entrypoint>.py`；参数、出处和限制见上述双语指南。
