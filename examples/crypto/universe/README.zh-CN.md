# 加密资产宇宙与跨资产研究

> **研究问题：** 透明的 point-in-time 候选排名或相对价值状态，能否在不变成组合分配器的前提下被检验？

## 案例地图

- `crypto_universe_opportunity_scan.py` / recorder / replay / response pair：流动性、已实现波动率、资金费率候选的持续性。
- `crypto_cross_asset_momentum_replay.py`、`crypto_volatility_adjusted_momentum_*`、`crypto_adaptive_cross_asset_replay.py`：
  排名、敏感性和按时间切分的样本外研究。
- `crypto_cross_asset_lead_lag_response_replay.py`：严格按时间戳对齐的领先资产收益与跟随资产未来响应研究。
- `crypto_cross_asset_correlation_response_replay.py`：严格按时间戳对齐的滚动收益相关性状态与后续相对响应研究。
- `crypto_altcoin_breadth_replay.py`、`crypto_global_market_regime_*`：市场参与度和提供方级全市场上下文。
- `crypto_pairs_mean_reversion_replay.py`：固定参数价差偏离和收敛诊断。
- `crypto_drawdown_recovery_response_replay.py`：运行高点回撤分桶，以及固定窗口的恢复/未来响应研究。
- `crypto_universe_delist_risk_monitor.py`：报价缺失/过期的数据质量护栏。
- `crypto_trend_template_response_replay.py`：价格-only 的 50/150/200 日趋势模板与 52 周区间响应研究。

## 快速开始

```bash
python3 examples/crypto/universe/crypto_universe_opportunity_scan.py \
  --exchange binance --market perp --interval 5m --min-score 2
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_walkforward.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --volatility-bars 8 --horizon-bars 8 \
  --train-fraction 0.7 --roundtrip-cost-bps 20
```

每次选择都只是 point-in-time 纸面列表。缺少成分、退市身份、前视偏差、宇宙定义、等权假设和成本门槛都必须显式记录。
“Top-k”不是资金分配指令，也不保证 Sharpe；breadth 案例是近似，不是官方指数。

完整命令矩阵和出处见 [`README.md`](README.md)。

```bash
python3 examples/crypto/universe/crypto_cross_asset_lead_lag_response_replay.py \
  --exchange binance --leader-symbol BTCUSDT --follower-symbol ETHUSDT \
  --interval 1h --days 90 --lookback-bars 1 --horizon-bars 1 \
  --leader-threshold-pct 0.10 --min-observations 20
```

Lead-lag 回放只使用两种资产的精确共同时间戳：领先资产的当前 K 线收益可见，跟随资产收益从该时间之后开始，
避免使用未来跟随数据。它检验的是关联，不是 Granger 因果、可交易 edge 或组合规则。

`crypto_cross_asset_correlation_response_replay.py` 与 lead-lag 互补：它不指定谁领先，而是在共同时间戳上用当前及历史收益计算 Pearson 相关性，
分成 low/middle/high co-movement，再比较下一个固定窗口的相对收益和绝对相对波动。它不暗示稳定 hedge ratio、均值回归、因果或可执行 pairs trade。
研究背景对照 [CME 的 BTC/ETH 相关性分析](https://www.cmegroup.com/insights/economic-research/2023/three-factors-driving-the-ether-bitcoin-price-nexus.html)
和 [滚动相关性研究](https://www.tandfonline.com/doi/full/10.1080/01605682.2026.2671242)，K 线字段对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

```bash
python3 examples/crypto/universe/crypto_cross_asset_correlation_response_replay.py \
  --exchange binance --first-symbol BTCUSDT --second-symbol ETHUSDT \
  --interval 1h --days 365 --correlation-window 30 --horizon-bars 6 \
  --low-correlation 0.30 --high-correlation 0.70 --min-observations 20
```

```bash
python3 examples/crypto/universe/crypto_trend_template_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1d --days 1825 \
  --sma-short-days 50 --sma-medium-days 150 --sma-long-days 200 \
  --slope-days 22 --range-days 252 --horizon-days 30 \
  --min-observations 5
```

```bash
python3 examples/crypto/universe/crypto_drawdown_recovery_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1d --days 3650 \
  --horizon-bars 90 --mild-drawdown-pct 10 \
  --moderate-drawdown-pct 20 --deep-drawdown-pct 40 \
  --min-observations 5
```

回撤回放只用当前 timestamp 之前已经出现的 K 线计算运行高点，然后按回撤区间报告未来收益、未来路径最差移动和窗口内是否回到此前高点。
它是可证伪的响应表，不是抄底、定投、资金分配或预测规则。研究线索来自
[CoinShares 的回撤背景说明](https://etp.coinshares.com/us/insights/research-data/bitcoins-drawdown-in-context/)；
[Cryptera 的回撤历史页](https://cryptera.app/bitcoin-drawdown-history)仅作为未经验证的描述性线索保留。

## 边界

本系列不再平衡资金、不路由订单、不管理仓位，也不把历史排名伪装成可交易容量。
