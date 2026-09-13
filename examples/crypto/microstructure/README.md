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

## Commands / 命令

```bash
python3 examples/crypto/microstructure/short_squeeze_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/exhaustion_short_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/liquidation_reversal_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/liquidation_reversal_replay.py \
  --exchange coinex --price-exchange binance --symbol BTCUSDT --limit 100 \
  --horizon-bars 3 --min-notional 100000
```
