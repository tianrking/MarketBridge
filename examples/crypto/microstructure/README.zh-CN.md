# 加密微结构、逼空与清算

> **研究问题：** 可观察的流量、深度、OI、清算或波动率状态，在不假装知道交易者意图的前提下，
> 是否对应不同的后续价格响应？

## 案例地图

| 证据 | 入口 |
|---|---|
| Confluence 监控 | `short_squeeze_monitor.py`、`exhaustion_short_monitor.py`、`liquidation_reversal_monitor.py`、`crypto_microstructure_monitor.py` |
| 流量与深度 | `crypto_flow_book_confirmation.py`、`crypto_footprint_imbalance_*`、`crypto_spot_perp_depth_gap_*`、`crypto_liquidity_stress_*` |
| 清算研究 | `crypto_liquidation_burst_*`、`crypto_liquidation_price_cluster_*`、`liquidation_reversal_replay.py` |
| 事件/技术回放 | `crypto_cvd_divergence_replay.py`、`crypto_trade_imbalance_bar_replay.py`、`crypto_vpin_response_replay.py`、`crypto_*vwap*`、`crypto_*breakout*`、`crypto_session_*`、`crypto_weekday_hour_effect_replay.py` |
| 衍生品拥挤 | `crypto_taker_oi_response_replay.py`、`crypto_account_ratio_oi_response_replay.py`、`crypto_derivatives_*`、`crypto_adl_risk_*` |

Recorder/replay pair 会把状态与报价一起冻结，再测量固定记录窗口的有符号或绝对收益。
ADL pair 只把 Binance rating 当作提供方上下文，不证明发生了 ADL，也不推断私人账户风险。

## 快速开始

```bash
python3 examples/crypto/microstructure/crypto_adl_risk_monitor.py \
  --symbol BTCUSDT --exchange binance
python3 examples/crypto/microstructure/crypto_adl_risk_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 60 \
  --output work/crypto-adl-risk-response.jsonl
python3 examples/crypto/microstructure/crypto_adl_risk_response_replay.py \
  --input work/crypto-adl-risk-response.jsonl --horizon-records 3 \
  --min-observations 5
```

完整命令保留在 [`README.md`](README.md)。首次轮询可能没有 OI 基线；清算 side、成交 side
和盘口语义都取决于提供方，必须保留在输出中。

## 证据规则

- Confluence 分数只是筛选观察，不是进出场信号。
- OI、flow、liquidation 或报价缺失时输出 `observe_only`，不能填零。
- 滚动 buffer、K 线近似或 aggregate long/short 比例不能揭示持仓归属、意图、dealer sign、
  潜在清算价位或因果关系。
- 成本、资金费率、借贷、延迟、滑点、排队和成交都不会被静默推断。

出处与覆盖说明在原目录文档中维护，包括 Binance [ADL Risk API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk)
和[公开清算研究线索](https://x.com/angustias87/status/2039147109228925373)。

## 边界

本系列不下单、不执行清算、不签名钱包，也不把公开 aggregate 指标解释为用户私人账户状态。
