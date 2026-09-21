# Microstructure / 微结构、逼空与清算

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

研究 order flow、CVD、盘口深度、OI、清算、波动率、区间突破和技术状态是否对应不同的后续
响应；不把聚合数据解释成交易者意图或真实止损池。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Squeeze/exhaustion | `short_squeeze_monitor.py`, `exhaustion_short_monitor.py` |
| Liquidations | `crypto_liquidation_*`, `crypto_crowded_liquidation_reversal_replay.py`, `liquidation_reversal_replay.py` |
| Flow/depth | `crypto_*flow*`, `crypto_*depth*`, `crypto_liquidity_*` |
| Taker-flow variance compression | `crypto_taker_flow_variance_compression_replay.py` |
| Technical response | `crypto_*breakout*`, `crypto_*vwap*`, `crypto_*volatility*` |
| Derivatives crowding | `crypto_*oi*`, `crypto_account_ratio_oi_response_replay.py` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/microstructure/short_squeeze_monitor.py \
  --symbol BTCUSDT --exchange binance
python3 examples/crypto/microstructure/crypto_liquidation_burst_replay.py \
  --exchange binance --symbol BTCUSDT --interval 15m \
  --days 30 --horizon-bars 8 --min-observations 5
```

详细案例地图、数据语义、清算保留窗口和出处见双语指南。实时 monitor 只打印证据；response
recorder/replay 才会把状态与同步报价配对。

## Boundary / 边界

清算事件不是完整 cascade ledger，OI 不是多空归属，盘口不是可成交深度。所有成本、延迟、
覆盖和执行假设必须显式记录；示例不下单、不管理仓位、不推断实盘 PnL。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/microstructure/<entrypoint>.py`；参数、出处和限制见上述双语指南。
