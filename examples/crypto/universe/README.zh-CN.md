# 加密资产宇宙与跨资产研究

> **研究问题：** 透明的 point-in-time 候选排名或相对价值状态，能否在不变成组合分配器的前提下被检验？

## 案例地图

- `crypto_universe_opportunity_scan.py` / recorder / replay / response pair：流动性、已实现波动率、资金费率候选的持续性。
- `crypto_cross_asset_momentum_replay.py`、`crypto_volatility_adjusted_momentum_*`、`crypto_adaptive_cross_asset_replay.py`：
  排名、敏感性和按时间切分的样本外研究。
- `crypto_altcoin_breadth_replay.py`、`crypto_global_market_regime_*`：市场参与度和提供方级全市场上下文。
- `crypto_pairs_mean_reversion_replay.py`：固定参数价差偏离和收敛诊断。
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
python3 examples/crypto/universe/crypto_trend_template_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1d --days 1825 \
  --sma-short-days 50 --sma-medium-days 150 --sma-long-days 200 \
  --slope-days 22 --range-days 252 --horizon-days 30 \
  --min-observations 5
```

## 边界

本系列不再平衡资金、不路由订单、不管理仓位，也不把历史排名伪装成可交易容量。
