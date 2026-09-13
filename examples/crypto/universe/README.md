# Universe and cross-asset research / 资产宇宙与跨资产研究

## English

These cases discover candidates rather than allocate capital. The universe
scanner joins liquidity, realized volatility and current funding. The
cross-asset replay ranks trailing returns across exact timestamp intersections
and compares the selected basket with an equal-weight benchmark over a fixed
horizon. Missing symbols and insufficient history remain visible.

## 中文

这些案例用于发现候选，不负责分配资金。Universe scanner 连接流动性、已实现波动率和当前
资金费率；跨资产回放在共同 timestamp 上排名历史收益，并将选中篮子与固定窗口的等权基准
比较。缺失标的和历史长度不足都会保留在结果里。

## Commands / 命令

```bash
python3 examples/crypto/universe/crypto_universe_opportunity_scan.py \
  --exchange binance --market perp --interval 5m --min-score 2
python3 examples/crypto/universe/crypto_cross_asset_momentum_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --horizon-bars 8 --top-k 1
```
