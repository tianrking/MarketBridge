# Carry and funding / 套利与资金费率

## English

This family studies relative prices and funding transfers without assuming a
fixed convergence date or executable hedge. The basis monitor joins spot/perp
quotes with current funding and only calls the result a research candidate
when the snapshot is fresh and the funding interval is known. The convergence
monitor/replay compares the same symbol across venues using explicit
point-in-time intervals. Missing intervals, borrow, transfer latency, margin,
fees and slippage remain evidence gaps, never zeros.

The convergence replay accepts `--paper-cost-bps-per-hour` and
`--min-net-spread-bps-per-hour`. These are explicit sensitivity hurdles: the
replay reports gross and after-cost differential persistence, but does not claim
they are exchange fees, borrow rates or executable hedge PnL.
It also carries each venue's historical funding `coverage_detail` into the
result, so a short provider page cannot be mistaken for a complete differential
window.

The basis recorder/replay is a separate falsifiable test: after a same-venue
basis observation is at least `min_z` standard deviations from its trailing
mean, does the absolute basis contract over the next `horizon` snapshots? It
reports contraction frequency and does not call that frequency carry PnL.

`crypto_funding_regime_replay.py` adds a funding-only persistence test: after
`min_run` consecutive observations remain beyond `min_funding_pct` in one
direction, does the next fixed price window move against that crowding proxy?
Positive funding is only a crowded-long proxy and negative funding only a
crowded-short proxy. The output is a forward-return hit rate, not funding
income, hedge PnL or an execution instruction. `funding_extremes.py` remains a
current-universe filter, while `funding_curve_demo.py` is a visualization
utility; neither is a backtest.

`crypto_positioning_regime_replay.py` joins funding, aggregate OI and perp price
at point-in-time timestamps, then reports the forward-return distribution for
each `price × OI × funding` regime. It is a state-matrix diagnostic, not a
long/short classifier: OI ownership, fills and hedge PnL remain unknown.

`crypto_funding_cross_section_replay.py` is a different case: it ranks a
caller-selected asset universe by point-in-time funding, keeps only fresh
funding observations, and compares the next-window returns of the lowest- and
highest-funding groups. It reports the low-minus-high spread with an optional
paper hurdle; it does not turn the ranking into a portfolio or hedge.

`crypto_cross_venue_price_gap_replay.py` isolates same-asset price
fragmentation: it aligns two venue candle series, detects an extreme log-price
gap relative to a frozen trailing mean, and measures subsequent contraction.
The result is intentionally not called an arbitrage opportunity because
simultaneous bid/ask fills, inventory, transfers and venue solvency are not
observed by this replay.

Provenance: the basis tests are motivated by the public [CryptoCred basis-trade
discussion on X](https://x.com/CryptoCred/status/1777720296297975952) and the
[CME-versus-spot basis example](https://x.com/0xscarlettw/status/1944584946670276938).
The funding persistence lead is cross-checked against the primary [Kraken
funding-rate strategy explanation](https://www.kraken.com/learn/futures-trading-funding-rate-strategy),
which describes funding as a positioning/crowding measure, and MarketBridge's
explicit funding schedule. These are research leads, not verified performance
claims.
The cross-venue differential lead is also informed by this public [funding
spread discussion on X](https://x.com/leondoteth/status/2012127303850213817).
The regime-matrix lead is informed by the public [OI/funding/price context
brief on X](https://x.com/ImCryptOpus/status/1949195275903410571).
The cross-sectional funding lead is also informed by the public [cross-venue
funding differential discussion on X](https://x.com/leondoteth/status/2012127303850213817)
and cross-checked against [Binance's official funding-history API documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).
The cross-venue gap decomposition is cross-checked against the academic
[Trading and Arbitrage in Cryptocurrency Markets](https://www.sciencedirect.com/science/article/pii/S0304405X19301746)
and [Arbitrage across different Bitcoin exchange venues](https://onlinelibrary.wiley.com/doi/10.1111/acfi.13102).
They motivate a price-fragmentation test, not an executable arbitrage claim.
The data semantics are cross-checked against [Binance's official open-interest
history documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info),
which describes bounded historical OI observations rather than trader-side ownership.
The funding/OI replay now forwards coverage metadata for funding, OI and price
history into its evidence output, so sparse timestamp overlap is visible in the
case result.

Useful inputs:

- `/v1/market/basis`
- `/v1/market/perpetual-funding`
- `/v1/history/candles?candle_type=funding_rate`

## 中文

这一系列研究现货/永续相对价格和资金费率转移，不假设固定收敛日期，也不假设可成交的
对冲。基差监控只有在快照新鲜、资金费率结算间隔已知时才报告研究候选；跨交易所收敛
监控/回放使用逐点时间间隔比较同一标的。缺失间隔、借币、转账延迟、保证金、手续费和
滑点都保持为证据缺口，绝不会当成零值。

收敛回放支持 `--paper-cost-bps-per-hour` 和 `--min-net-spread-bps-per-hour`，用于显式纸面敏感性
门槛。它会同时输出 gross 与扣除该门槛后的差异持续性，但不会把门槛冒充交易所手续费、借贷成本或
可成交对冲 PnL。

基差录制/回放是独立的可证伪测试：当同一交易所的基差相对滚动均值偏离至少
`min_z` 个标准差后，未来 `horizon` 个快照的绝对基差是否收缩？输出的是收缩频率，
不会把它冒充成套利 PnL。

`crypto_funding_regime_replay.py` 进一步做只用资金费率的持续性检验：连续
`min_run` 个观测同方向超过 `min_funding_pct` 后，未来固定价格窗口是否朝拥挤一侧的
反方向移动？正费率只作为多头拥挤代理，负费率只作为空头拥挤代理；输出是未来收益方向命中率，
不是资金费收入、对冲 PnL 或执行指令。`funding_extremes.py` 只是当前市场筛选，
`funding_curve_demo.py` 只是可视化，二者都不是回测。

`crypto_positioning_regime_replay.py` 在逐点时间上连接资金费率、聚合 OI 和永续价格，输出每个
`价格 × OI × 资金费率` 状态的未来收益分布。它是状态矩阵诊断，不是多空分类器；OI 归属、成交和
对冲 PnL 仍然未知。

`crypto_funding_cross_section_replay.py` 是不同的横截面案例：按逐点资金费率给调用者选择的资产宇宙
排名，只保留新鲜费率，再比较低费率组与高费率组的下一窗口收益。它输出低减高的差异并允许加入纸面
门槛，但不会把排名变成组合或对冲。

出处：基差测试思路来自公开的 [CryptoCred 基差交易讨论](https://x.com/CryptoCred/status/1777720296297975952)
和 [CME 与现货基差示例](https://x.com/0xscarlettw/status/1944584946670276938)。资金费率持续性线索
另外对照了一级资料 [Kraken 资金费率策略说明](https://www.kraken.com/learn/futures-trading-funding-rate-strategy)，
以及 MarketBridge 返回的明确结算间隔。它们都是研究线索，不是已经验证的收益声明。
跨交易所差异线索也参考了公开的 [资金费率价差讨论](https://x.com/leondoteth/status/2012127303850213817)。
状态矩阵线索也参考了公开的 [OI/资金费率/价格上下文简报](https://x.com/ImCryptOpus/status/1949195275903410571)。
横截面资金费率线索也参考了公开的 [跨交易所资金费率差异讨论](https://x.com/leondoteth/status/2012127303850213817)，
并对照 [Binance 官方资金费率历史 API 文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)。

主要接口：

- `/v1/market/basis`
- `/v1/market/perpetual-funding`
- `/v1/history/candles?candle_type=funding_rate`

## Commands / 命令

```bash
python3 examples/crypto/carry/basis_carry_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/carry/funding_convergence_monitor.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit --iterations 3
python3 examples/crypto/carry/funding_convergence_replay.py \
  --symbol BTCUSDT --exchanges binance,bybit --days 7 --limit 200 \
  --paper-cost-bps-per-hour 0.25 --min-net-spread-bps-per-hour 0.5
python3 examples/crypto/carry/crypto_basis_recorder.py \
  --symbol BTCUSDT --exchanges binance,okx --iterations 120 --interval-secs 30 \
  --output work/crypto-basis.jsonl
python3 examples/crypto/carry/crypto_basis_replay.py \
  --input work/crypto-basis.jsonl --symbol BTCUSDT \
  --lookback 20 --horizon 3 --min-z 2.0
python3 examples/crypto/carry/crypto_funding_oi_replay.py \
  --symbol BTCUSDT --funding-exchange binance --oi-exchange binance \
  --price-exchange binance --days 7
python3 examples/crypto/carry/crypto_funding_regime_replay.py \
  --symbol BTCUSDT --funding-exchange binance --price-exchange binance \
  --days 14 --min-funding-pct 0.01 --min-run 3 --horizon-bars 3
python3 examples/crypto/carry/crypto_positioning_regime_replay.py \
  --symbol BTCUSDT --funding-exchange binance --oi-exchange binance \
  --price-exchange binance --days 7 --price-interval 5m \
  --lookback-bars 3 --horizon-bars 3 --min-observations 3
python3 examples/crypto/carry/crypto_funding_cross_section_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --funding-exchange binance \
  --price-exchange binance --interval 1h --days 14 --top-k 1 \
  --min-dispersion-bps 1 --paper-cost-bps 10 --min-edge-bps 0
python3 examples/crypto/carry/crypto_cross_venue_price_gap_replay.py \
  --exchange-a binance --exchange-b okx --symbol BTCUSDT --market spot \
  --interval 5m --lookback-bars 24 --horizon-bars 6 --entry-z 2 \
  --paper-cost-bps 10 --min-contraction-bps 0
python3 examples/crypto/carry/funding_extremes.py \
  --exchange binance --min-pct -2 --max-pct -0.1
python3 examples/crypto/carry/funding_curve_demo.py \
  --symbol BTCUSDT --days 30 --no-png
```
