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
- `crypto_microstructure_monitor.py`: top-of-book imbalance with funding context.
- `crypto_flow_book_confirmation.py`: taker flow confirms or rejects L2 pressure.
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

## 中文

这些观察器组合资金费率、OI 变化、现货/永续订单流、盘口深度、价格上下文和清算事件，
输出的是共振证据，不是入场信号。第一次轮询没有 OI 基线是有意设计；不同交易所的清算
side 语义不一定相同，因此回放会保留来源和覆盖元数据，缺失数据降级为 `observe_only`。

案例包括：

- `short_squeeze_monitor.py`：负资金费率 + OI 上升 + 现货/永续订单流背离。
- `exhaustion_short_monitor.py`：正资金费率 + 冲高失败 + OI 下降 + 买盘变弱。
- `liquidation_reversal_monitor.py`：卖方清算 + OI 下降 + CVD 转正 + 价格恢复。
- `crypto_microstructure_monitor.py`：盘口失衡结合资金费率上下文。
- `crypto_flow_book_confirmation.py`：订单流确认或否定 L2 压力。
- `liquidity_stress_monitor.py`：目标名义金额的可执行冲击 + 点差 + 短周期 EWMA 波动率。

流动性压力案例是风险上下文观察器：它回答“现在以指定名义金额退出是否昂贵”，
而不是预测涨跌。冲击、点差、波动率三项中至少两项达到阈值才报告
`liquidity_stress`；盘口或 K 线缺失会明确保留，不会填成零。

出处：实现拆解自 [Pine Analytics / FlyingTulip 在 X 的执行风险讨论](https://x.com/PineAnalytics/status/1974474638093590994)，
原文强调真实盘口深度、目标规模滑点和短周期 EWMA 波动率。这里是独立、可证伪的
研究假设，不代表对原文或盈利能力的背书。

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
python3 examples/liquidation_reversal_replay.py \
  --exchange coinex --price-exchange binance --symbol BTCUSDT --limit 100 \
  --horizon-bars 3 --min-notional 100000
```
