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

出处：基差测试思路来自公开的 [CryptoCred 基差交易讨论](https://x.com/CryptoCred/status/1777720296297975952)
和 [CME 与现货基差示例](https://x.com/0xscarlettw/status/1944584946670276938)。资金费率持续性线索
另外对照了一级资料 [Kraken 资金费率策略说明](https://www.kraken.com/learn/futures-trading-funding-rate-strategy)，
以及 MarketBridge 返回的明确结算间隔。它们都是研究线索，不是已经验证的收益声明。
跨交易所差异线索也参考了公开的 [资金费率价差讨论](https://x.com/leondoteth/status/2012127303850213817)。

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
python3 examples/crypto/carry/funding_extremes.py \
  --exchange binance --min-pct -2 --max-pct -0.1
python3 examples/crypto/carry/funding_curve_demo.py \
  --symbol BTCUSDT --days 30 --no-png
```
