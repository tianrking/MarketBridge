# Carry and funding / 资金费率与套利

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

研究 basis、funding、跨 venue 报价/盘口差、Coinbase premium 和三角报价一致性。
核心问题是：一个可观察的状态是否持续，或在固定窗口后呈现可测量响应？这不是收益保证，
也不是可执行对冲。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Basis & funding | `basis_carry_monitor.py`, `crypto_funding_*` |
| Cross-venue | `crypto_cross_venue_*`, `crypto_coinbase_premium_*` |
| Triangular | `crypto_triangular_arbitrage_*` |
| Response studies | `*_response_recorder.py`, `*_response_replay.py` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance --threshold 0.8
python3 examples/crypto/carry/crypto_funding_band_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 600 \
  --output work/crypto-funding-band-response.jsonl
python3 examples/crypto/carry/crypto_funding_band_response_replay.py \
  --input work/crypto-funding-band-response.jsonl \
  --horizon-records 3 --min-observations 5
```

先运行 monitor，再用 recorder 生成独立 JSONL，最后运行 replay。详细命令、字段解释、出处
和参数敏感性见 [English guide](README.en.md) 与 [中文指南](README.zh-CN.md)。

## Interpretation / 解释

- 缺少 funding interval、provider band、借贷、手续费、库存、转账延迟、滑点或成交覆盖时，
  结果只能称为 gross observation。
- `horizon-records` 是记录条数，不自动等于小时数；先检查时间戳和采样频率。
- 公开收盘价不是成交价；basis 收敛也不等于 carry PnL。

## Boundary / 边界

只读观察和纸面回放；不借币、不对冲、不转账、不签名钱包、不发送订单。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/carry/<entrypoint>.py`；参数、出处和限制见上述双语指南。
