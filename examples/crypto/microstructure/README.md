# Microstructure, squeeze and liquidation / 微结构、逼空与清算

## English

These observers combine funding, OI change, spot/perp order flow, book depth,
price context and liquidation events. They are confluence reports, not entry
signals. The first polling cycle intentionally has no OI change baseline. A
venue-specific liquidation side is not universal, so the replay keeps source
and coverage metadata visible and downgrades missing data to `observe_only`.

Cases:

- `short_squeeze_monitor.py`: negative funding + rising OI + spot/perp flow divergence.
- `exhaustion_short_monitor.py`: positive funding + failed highs + falling OI + weak bids.
- `liquidation_reversal_monitor.py`: sell-side liquidation + falling OI + positive CVD + recovery.
- `crypto_liquidation_burst_replay.py`: rolling liquidation-notional threshold versus forward absolute price movement.
- `crypto_microstructure_monitor.py`: top-of-book imbalance with funding context.
- `crypto_flow_book_confirmation.py`: taker flow confirms or rejects L2 pressure.
- `crypto_spot_perp_depth_gap_monitor.py`: compares same-venue spot/perp target-size depth and impact.
- `crypto_spot_perp_depth_gap_recorder.py` / `crypto_spot_perp_depth_gap_replay.py`: test whether that gap persists across snapshots.
- `crypto_volatility_breakout_replay.py`: compressed range plus volume confirmation versus forward returns.
- `crypto_session_filter.py`: VWAP/EMA/MACD/volume session-window filter replay.
- `liquidity_stress_monitor.py`: target-size executable impact + spread + short-horizon EWMA volatility.

The liquidity-stress case is a risk-context monitor: it asks whether a chosen
notional is expensive to unwind right now, rather than predicting direction.
It requires two of three stress components (impact, spread, volatility) before
reporting `liquidity_stress`; missing depth or candles remains explicit.

Provenance: the decomposition follows the public [Pine Analytics / FlyingTulip
execution-aware risk discussion on X](https://x.com/PineAnalytics/status/1974474638093590994),
which emphasizes real order-book depth, target-size slippage and short-horizon
EWMA volatility. The implementation is an independently testable hypothesis,
not an endorsement or a claim that the post's idea is profitable.

The liquidation-burst replay is deliberately different from the single-event
reversal monitor: it aggregates all public liquidation notional over a rolling
window, compares the next price movement with ordinary candle windows, and
keeps side labels as metadata only. It does not assume that a venue's `sell`
label proves a long liquidation.

Provenance: [CryptoData's public liquidation-threshold discussion on X](https://x.com/TheCryptoData/status/1948466627365769584)
is treated as an unverified research lead; the replay tests the threshold and
reports the data-coverage limits instead of repeating the claim.

The spot/perp depth-gap monitor is an execution-risk observation motivated by
[a public discussion of the spot/perp depth gap on X](https://x.com/ciaobelindazhou/status/2031929849850273955).
It tests the claim with a target-size snapshot and current basis context; it
does not assume that deeper perp liquidity makes a hedge executable.

## 中文

这些观察器组合资金费率、OI 变化、现货/永续订单流、盘口深度、价格上下文和清算事件，
输出的是共振证据，不是入场信号。第一次轮询没有 OI 基线是有意设计；不同交易所的清算
side 语义不一定相同，因此回放会保留来源和覆盖元数据，缺失数据降级为 `observe_only`。

案例包括：

- `short_squeeze_monitor.py`：负资金费率 + OI 上升 + 现货/永续订单流背离。
- `exhaustion_short_monitor.py`：正资金费率 + 冲高失败 + OI 下降 + 买盘变弱。
- `liquidation_reversal_monitor.py`：卖方清算 + OI 下降 + CVD 转正 + 价格恢复。
- `crypto_liquidation_burst_replay.py`：滚动清算名义金额阈值与未来绝对价格波动对比。
- `crypto_microstructure_monitor.py`：盘口失衡结合资金费率上下文。
- `crypto_flow_book_confirmation.py`：订单流确认或否定 L2 压力。
- `crypto_spot_perp_depth_gap_monitor.py`：比较同交易所现货/永续的目标规模深度与冲击。
- `crypto_spot_perp_depth_gap_recorder.py` / `crypto_spot_perp_depth_gap_replay.py`：检验该深度差是否在多个快照中持续。
- `crypto_volatility_breakout_replay.py`：压缩区间突破结合成交量确认，并测量未来收益。
- `crypto_session_filter.py`：VWAP/EMA/MACD/成交量的时段过滤回放。
- `liquidity_stress_monitor.py`：目标名义金额的可执行冲击 + 点差 + 短周期 EWMA 波动率。

流动性压力案例是风险上下文观察器：它回答“现在以指定名义金额退出是否昂贵”，
而不是预测涨跌。冲击、点差、波动率三项中至少两项达到阈值才报告
`liquidity_stress`；盘口或 K 线缺失会明确保留，不会填成零。

出处：实现拆解自 [Pine Analytics / FlyingTulip 在 X 的执行风险讨论](https://x.com/PineAnalytics/status/1974474638093590994)，
原文强调真实盘口深度、目标规模滑点和短周期 EWMA 波动率。这里是独立、可证伪的
研究假设，不代表对原文或盈利能力的背书。

清算 burst 回放与单次事件反转监控不同：它在滚动窗口内聚合所有公开清算名义金额，
再和普通 K 线窗口的未来价格波动比较；side 只作为元数据保留，不假设交易所的
`sell` 一定代表多头清算。

出处：[CryptoData 在 X 的清算阈值讨论](https://x.com/TheCryptoData/status/1948466627365769584)
只是未经验证的研究线索；回放会检验阈值，并把覆盖范围限制明确输出，而不是复述结论。

现货/永续深度差监控的研究线索来自[公开 X 讨论](https://x.com/ciaobelindazhou/status/2031929849850273955)。
它用目标规模盘口和当前 basis 做执行风险观察，不假设永续深度更深就代表对冲一定可成交。

## Commands / 命令

```bash
python3 examples/crypto/microstructure/short_squeeze_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/exhaustion_short_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/liquidation_reversal_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/liquidity_stress_monitor.py \
  --symbol BTCUSDT --exchange binance --liquidity-target-notional 10000 \
  --liquidity-volatility-bars 60 --iterations 3
python3 examples/crypto/microstructure/crypto_liquidation_burst_replay.py \
  --exchange okx --price-exchange okx --symbol BTCUSDT \
  --threshold-notional 1000000 --window-hours 24 --horizon-bars 12
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_monitor.py \
  --symbol BTCUSDT --exchange binance --target-notional 10000 \
  --min-depth-ratio 2.0 --min-impact-improvement-bps 5
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --output work/crypto-spot-perp-depth.jsonl
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_replay.py \
  --input work/crypto-spot-perp-depth.jsonl --min-depth-ratio 2.0 --min-run 3
python3 examples/crypto/microstructure/crypto_volatility_breakout_replay.py \
  --exchange binance --symbol BTCUSDT --interval 5m --days 7
python3 examples/crypto/microstructure/crypto_session_filter.py \
  --exchange binance --market perp --symbol BTCUSDT --interval 1m --limit 60
python3 examples/liquidation_reversal_replay.py \
  --exchange coinex --price-exchange binance --symbol BTCUSDT --limit 100 \
  --horizon-bars 3 --min-notional 100000
```
