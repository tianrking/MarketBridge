# 加密微结构、逼空与清算

> **研究问题：** 可观察的流量、深度、OI、清算或波动率状态，在不假装知道交易者意图的前提下，
> 是否对应不同的后续价格响应？

## 案例地图

| 证据 | 入口 |
|---|---|
| Confluence 监控 | `short_squeeze_monitor.py`、`exhaustion_short_monitor.py`、`liquidation_reversal_monitor.py`、`crypto_microstructure_monitor.py` |
| 流量与深度 | `crypto_flow_book_confirmation.py`、`crypto_footprint_imbalance_*`、`crypto_spot_perp_depth_gap_*`、`crypto_liquidity_stress_*` |
| 双侧墙体 | `crypto_liquidity_sandwich_monitor.py`、`crypto_liquidity_sandwich_response_recorder.py`、`crypto_liquidity_sandwich_response_replay.py` |
| 清算研究 | `crypto_liquidation_burst_*`、`crypto_liquidation_price_cluster_*`、`liquidation_reversal_replay.py` |
| 事件/技术回放 | `crypto_cvd_divergence_replay.py`、`crypto_trade_imbalance_bar_replay.py`、`crypto_vpin_response_replay.py`、`crypto_*vwap*`、`crypto_*breakout*`、`crypto_session_*`、`crypto_weekly_rsi_cross_response_replay.py`、`crypto_weekday_hour_effect_replay.py` |
| 衍生品拥挤 | `crypto_taker_oi_response_replay.py`、`crypto_oi_price_divergence_response_replay.py`、`crypto_account_ratio_oi_response_replay.py`、`crypto_derivatives_*`、`crypto_adl_risk_*` |

Recorder/replay pair 会把状态与报价一起冻结，再测量固定记录窗口的有符号或绝对收益。
ADL pair 只把 Binance rating 当作提供方上下文，不证明发生了 ADL，也不推断私人账户风险。
实时清算存储现在按 venue/symbol 保留有界的近期事件窗口，不再覆盖上一条事件。给
`crypto_liquidation_burst_response_recorder.py` 传 `--source market` 可归档 Binance、Bybit、BitMEX、Gate
等已启用实时 feed；`--source history` 仍使用 OKX/CoinEx 有界历史路径。保留窗口不是完整历史账本，feed 缺口必须保留。
liquidity-sandwich pair 只检验一个更窄的公开 X 假设：当买卖两侧近盘口深度都明显、点差较窄时，
后续 BTC 绝对波动是否不同于普通快照；不会把显示深度称为持续墙体，也不推导区间交易机会。

`crypto_weekly_rsi_cross_response_replay.py` 是独立的收盘价研究：在 `1w` K 线上计算明确实现的
RSI(14) 与其 14 周简单均线，比较上穿/下穿后固定周数的有符号收益和路径最低收益。它不会继承平台私有指标口径，
也不会把 X 帖子里的回撤描述变成预测。

```bash
python3 examples/crypto/microstructure/crypto_weekly_rsi_cross_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1w --days 3650 \
  --rsi-period 14 --rsi-sma-period 14 --horizon-weeks 4 \
  --min-observations 3
```

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
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_monitor.py \
  --symbol BTCUSDT --exchange binance --depth-band-bps 10 \
  --min-side-depth-notional 100000 --min-symmetry-ratio 0.5
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --output work/crypto-liquidity-sandwich-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_replay.py \
  --input work/crypto-liquidity-sandwich-response.jsonl \
  --horizon-records 3 --min-observations 5
```

完整命令保留在 [`README.md`](README.md)。首次轮询可能没有 OI 基线；清算 side、成交 side
和盘口语义都取决于提供方，必须保留在输出中。

OI/价格象限回放会把每根价格 K 线与不晚于该时间的最新 OI 对齐，保留 OI 年龄和提供方单位，
分类为价格上升/OI 上升、价格上升/OI 下降、价格下降/OI 上升、价格下降/OI 下降四种可观察状态，
再比较后续有符号和绝对收益。标签不证明逼空、新空头、长仓清算或交易者意图。

```bash
python3 examples/crypto/microstructure/crypto_oi_price_divergence_response_replay.py \
  --symbol BTCUSDT --price-exchange binance --oi-exchange okx \
  --price-interval 5m --oi-interval 5m --days 2 \
  --lookback-bars 3 --horizon-bars 3 --min-observations 5
```

分解动机来自 [TheCryptoData 的公开 OI 与清算讨论](https://x.com/TheCryptoData/status/1948466627365769584)，
字段语义同时参考[OKX 合约 OI 文档](https://www.okx.com/docs-v5/en/#rest-api-trading-data-get-contracts-open-interest-and-volume)
和现有 Binance 历史接口。它们只是研究输入，不证明 divergence 交易具有收益。

```bash
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_recorder.py \
  --source market --exchange binance --price-exchange binance \
  --symbol BTCUSDT --iterations 120 --interval-secs 30 \
  --output work/crypto-live-liquidation-burst-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_replay.py \
  --input work/crypto-live-liquidation-burst-response.jsonl \
  --window-hours 1 --horizon-records 12 --threshold-notional 1000000 \
  --cooldown-records 12 --min-observations 3
```

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
