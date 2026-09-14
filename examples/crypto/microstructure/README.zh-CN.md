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

`crypto_liquidity_sweep_response_replay.py` 检验公开“流动性扫损/收回”叙事中可以从 OHLCV 观察到的子集：当前 K 线刺破前序回看窗口的高点或低点，
随后收盘重新穿回该水平，并且实体/波动达到阈值；然后报告按方向对齐的未来收益。这不能证明真实止损流动性、潜在 liquidity pool、CISD、displacement 意图或可执行形态。

```bash
python3 examples/crypto/microstructure/crypto_liquidity_sweep_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 15m \
  --days 15 --lookback-bars 20 --sweep-buffer-bps 0 \
  --min-body-fraction 0.50 --min-range-bps 5 \
  --horizon-bars 8 --paper-cost-bps 10 --min-observations 5
```

研究线索来自 [KM Trading 在 X 的 setup 拆解](https://x.com/KMTrading_SMC/status/2032428981040103847)。
MarketBridge 的字段语义对照 [Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)，
并主动收窄 X 帖子术语：只检验前序区间突破、收回和 K 线几何，不把术语变成订单执行规则。

`crypto_atr_regime_response_replay.py` 是独立的波动率上下文研究：计算简单平均真实波幅（ATR），用当前时点以前的滚动分布把状态分为压缩、普通和扩张，
再比较各状态之后的有符号收益、绝对收益和路径风险。它不预测方向、不计算仓位、不设置止损，也不执行交易。研究线索来自
[X 上的 regime/ATR 讨论](https://x.com/viviennaBTC/status/2037854988442235187)，计算口径对照
[Binance Academy 的 ATR 说明](https://www.binance.com/en/square/post/510812)。

```bash
python3 examples/crypto/microstructure/crypto_atr_regime_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --atr-period 14 --regime-lookback 96 \
  --low-quantile 0.20 --high-quantile 0.80 \
  --horizon-bars 8 --min-observations 5
```

`crypto_breakout_retest_response_replay.py` 检验与 sweep 相反的延续假设：收盘突破前序区间后，在限定窗口内触及被突破水平，并重新收在突破方向一侧；未来窗口从回踩收盘开始，确保回踩先被观察再测量响应。
它不推断真实支撑/阻力、挂单、成交量确认或成交。研究线索来自
[Rekt Capital 在 X 的 BTC 突破/回踩讨论](https://x.com/rektcapital/status/1850982324621676715)，并对照
[Binance Academy 的加密突破说明](https://www.binance.com/en/academy/articles/a-beginners-guide-to-swing-trading-cryptocurrency)。

```bash
python3 examples/crypto/microstructure/crypto_breakout_retest_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --lookback-bars 24 --breakout-buffer-bps 2 \
  --retest-window 8 --retest-tolerance-bps 15 --horizon-bars 8 \
  --paper-cost-bps 10 --min-observations 5
```

`crypto_ichimoku_cloud_response_replay.py` 实现 point-in-time Ichimoku 响应表：同时观察价格相对云层的位置、Tenkan/Kijun 关系、云颜色和 Chikou 对比；当前可见的 Senkou 云值只读取位移以前已经计算出的历史线，避免把未来投影云层当成当前已知数据。
这些状态只是描述性分组，不包含预测、资金分配、止损或执行。研究线索来自未经验证的
[X 上 Ichimoku/云层讨论](https://x.com/Invst_Informant/status/2014788740992929906)，公式对照
[Binance Academy 的 Ichimoku 说明](https://www.binance.com/en/academy/articles/ichimoku-clouds-explained)。

```bash
python3 examples/crypto/microstructure/crypto_ichimoku_cloud_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --conversion-period 9 --base-period 26 \
  --span-b-period 52 --displacement 26 --horizon-bars 6 \
  --min-observations 5
```

`crypto_rsi_bollinger_extreme_response_replay.py` 是组合极值案例：把 RSI-only 极值、Bollinger-only 越界、共同超买/超卖以及普通 K 线分开，再比较后续响应。
它不假设极值必然反转；强趋势持续、指标口径和参数敏感性都会保留。研究线索来自
[X 上 BTC RSI + 上轨讨论](https://x.com/MichaelMOTTCM/status/1944846581611814956)，定义对照
[Binance RSI 词典](https://www.binance.com/en/academy/glossary/relative-strength-index)
和 [Bollinger Bands 说明](https://www.binance.com/en/square/post/42841)。

```bash
python3 examples/crypto/microstructure/crypto_rsi_bollinger_extreme_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --rsi-period 14 --band-period 20 --deviations 2 \
  --overbought 70 --oversold 30 --horizon-bars 12 \
  --min-observations 5
```

`crypto_fibonacci_retracement_response_replay.py` 只检验一个更窄的、point-in-time 回撤假设：
在当前 K 线以前的回看窗口内选择高点和低点，按两者的时间顺序确定上涨或下跌方向，
再把当前收盘价分到 38.2%、50% 或 61.8% 附近（另设区间内和区间外对照）。锚点窗口严格在当前 K 线之前结束，
因此不会把未来 swing 泄漏到特征；输出比较后续有符号、方向对齐和绝对收益。
它不声称 Fibonacci 水平必然是支撑/阻力，也不生成入场、止损或下单规则。研究线索来自
[X 上 WIF 的 Fibonacci 讨论](https://x.com/CryptoJournaal/status/2026693734063075685)，定义对照
[Binance Academy Fibonacci 指南](https://www.binance.com/en/academy/articles/a-guide-to-mastering-fibonacci-retracement)
和 [Binance 词典](https://www.binance.com/en/academy/glossary/fibonacci-retracement)。

```bash
python3 examples/crypto/microstructure/crypto_fibonacci_retracement_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --lookback-bars 90 --level-tolerance 0.03 \
  --horizon-bars 6 --min-observations 5
```

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
