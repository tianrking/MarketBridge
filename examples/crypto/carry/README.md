# Carry and funding / 套利与资金费率

## English

This family studies relative prices and funding transfers without assuming a
fixed convergence date or executable hedge. The basis monitor joins spot/perp
quotes with current funding and only calls the result a research candidate
when the snapshot is fresh and the funding interval is known. The convergence
monitor/replay compares the same symbol across venues using explicit
point-in-time intervals. Missing intervals, borrow, transfer latency, margin,
fees and slippage remain evidence gaps, never zeros.

Useful inputs:

- `/v1/market/basis`
- `/v1/market/perpetual-funding`
- `/v1/history/candles?candle_type=funding_rate`

## 中文

这一系列研究现货/永续相对价格和资金费率转移，不假设固定收敛日期，也不假设可成交的
对冲。基差监控只有在快照新鲜、资金费率结算间隔已知时才报告研究候选；跨交易所收敛
监控/回放使用逐点时间间隔比较同一标的。缺失间隔、借币、转账延迟、保证金、手续费和
滑点都保持为证据缺口，绝不会当成零值。

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
  --symbol BTCUSDT --exchanges binance,bybit --days 7 --limit 200
python3 examples/crypto/carry/crypto_funding_oi_replay.py \
  --symbol BTCUSDT --funding-exchange binance --oi-exchange binance \
  --price-exchange binance --days 7
```
