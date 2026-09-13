# Universe and cross-asset research / 资产宇宙与跨资产研究

## English

These cases discover candidates rather than allocate capital. The universe
scanner joins liquidity, realized volatility and current funding. The
cross-asset replay ranks trailing returns across exact timestamp intersections
and compares the selected basket with an equal-weight benchmark over a fixed
horizon. Missing symbols and insufficient history remain visible.

`crypto_universe_opportunity_recorder.py` / `crypto_universe_opportunity_replay.py`
freeze the scanner's candidate sets and test whether the top-k universe remains
present across snapshots. This is persistence evidence only; it does not turn a
ranking into an allocation or execution signal.

The volatility-adjusted replay is a separate ranking test: it divides each
asset's trailing return by its trailing per-bar realized volatility before
selecting the top basket. Zero-volatility assets are excluded rather than
assigned an infinite score. This tests risk-adjusted ranking; it does not
allocate capital or promise a Sharpe ratio.

Both the replay and sweep accept `--roundtrip-cost-bps` as a transparent paper
hurdle. It subtracts a fixed relative cost from the basket edge; it is not a
venue-specific fee, queue, fill or capacity model.

`crypto_volatility_adjusted_momentum_sweep.py` runs a bounded grid over the
lookback, volatility and forward-horizon windows after fetching each symbol
once. It exposes in-sample sensitivity and explicitly labels the best row as
descriptive only; a time-held-out, cost-aware replay is still required.

`crypto_volatility_adjusted_momentum_walkforward.py` performs that chronological
holdout for one selected parameter set. Warm-up bars before the split are used
only to form test features; the test score is never used to choose parameters.

Provenance: [RoboNet's public multi-asset strategy discussion on X](https://x.com/RoboNetHQ/status/2024893544520143012)
motivates the volatility-adjusted comparison, while [CME's crypto
diversification study](https://www.cmegroup.com/articles/2025/diversifying-crypto-portfolios-with-xrp-and-sol.html)
documents that major crypto assets have materially different volatility. Both
are inputs to a falsifiable replay, not evidence of a guaranteed edge.

The candidate-discovery framing is also informed by the public [multi-asset
perpetuals discussion by RoboNet](https://x.com/RoboNetHQ/status/2024893544520143012);
it is treated as an unverified research lead.

## 中文

这些案例用于发现候选，不负责分配资金。Universe scanner 连接流动性、已实现波动率和当前
资金费率；跨资产回放在共同 timestamp 上排名历史收益，并将选中篮子与固定窗口的等权基准
比较。缺失标的和历史长度不足都会保留在结果里。

波动率调整回放是独立的排名测试：先用历史收益除以逐 K 线已实现波动率，再选择排名靠前
的篮子。零波动标的会被排除，而不是赋予无穷大分数。它测试风险调整后的排名，不分配资金，
也不承诺 Sharpe 比率。

回放和扫描都支持 `--roundtrip-cost-bps` 透明纸面成本门槛：它从篮子 edge 中扣除固定相对
成本，但不是交易所费率、队列、成交或容量模型。

`crypto_volatility_adjusted_momentum_sweep.py` 在只请求一次每个标的历史数据后，扫描回看、
波动率和前瞻窗口的有限网格。它用于暴露样本内敏感性，并明确把最佳行标记为描述性结果；
仍需时间切分、成本感知的样本外回放。

`crypto_volatility_adjusted_momentum_walkforward.py` 对选定参数执行按时间排序的训练/测试
切分。测试段只使用切分前的预热 K 线和切分后的当前数据，测试结果不会反过来挑参数。

`crypto_universe_opportunity_recorder.py` / `crypto_universe_opportunity_replay.py` 会冻结 scanner
候选集合，检验 top-k 标的是否跨快照持续出现。这只是候选持续性证据，不会把排名变成资金分配或执行信号。

出处：[RoboNet 在 X 的多资产策略讨论](https://x.com/RoboNetHQ/status/2024893544520143012)
提供了波动率调整的研究线索；[CME 的加密资产分散研究](https://www.cmegroup.com/articles/2025/diversifying-crypto-portfolios-with-xrp-and-sol.html)
说明主要加密资产的波动率确实不同。两者只是可证伪回放的输入，不代表保证收益。

## Commands / 命令

```bash
python3 examples/crypto/universe/crypto_universe_opportunity_scan.py \
  --exchange binance --market perp --interval 5m --min-score 2
python3 examples/crypto/universe/crypto_universe_opportunity_recorder.py \
  --exchange binance --market perp --interval 5m \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --min-score 2 \
  --iterations 30 --interval-secs 30 --output work/crypto-universe-opportunities.jsonl
python3 examples/crypto/universe/crypto_universe_opportunity_replay.py \
  --input work/crypto-universe-opportunities.jsonl --top-k 3 --min-run 3
python3 examples/crypto/universe/crypto_cross_asset_momentum_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --horizon-bars 8 --top-k 1
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --volatility-bars 8 --horizon-bars 8 --top-k 1 \
  --roundtrip-cost-bps 20
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_sweep.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 4,8,12 --volatility-bars 4,8,12 \
  --horizon-bars 4,8 --top-k 1 --roundtrip-cost-bps 20
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_walkforward.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --volatility-bars 8 --horizon-bars 8 \
  --train-fraction 0.7 --roundtrip-cost-bps 20
```
