# 加密期权与波动率

> **研究问题：** 在不虚构 dealer 持仓、对冲执行或期权 PnL 的前提下，可观察的 IV 曲面状态是否对应
> 不同的后续市场响应？

## 案例地图

- `crypto_options_skew_*` 与期限结构回放：翼部 IV、近/远端 ATM slope 的持续性或 BTC 响应。
- `crypto_options_panic_regime_response_replay.py`：联合高 ATM IV 与正的
  put-minus-call skew 响应研究。
- `crypto_options_put_call_oi_*`：把提供方 OI 构成为 defensive、call-dominant 或 balanced 上下文。
- `crypto_options_gamma_*`、`crypto_options_max_pain_*`、`crypto_options_bull_call_spread_*`：
  透明的曲面/报价几何研究。
- `crypto_options_vrp_*`、`crypto_deribit_*` 和历史波动率回放：IV 与 realized volatility 对照及 Deribit index 响应。

## 快速开始

```bash
python3 examples/crypto/options/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30
python3 examples/crypto/options/crypto_options_skew_recorder.py \
  --currency BTC --venue deribit --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew.jsonl
python3 examples/crypto/options/crypto_options_skew_replay.py \
  --input work/crypto-options-skew.jsonl --min-run 3
```

Response pair 会加入同步 BTC 报价并使用 `--horizon-records`；它测量曲面状态关联，不是期权收益。
到期身份、缺失 greeks、moneyness bucket、报价新鲜度和覆盖都必须保留。

Panic-regime 回放明确沿用 MarketBridge 约定：
`put_call_skew_iv = put_iv - call_iv`，所以正值表示 put 翼 IV 相对更高。
它会把联合状态与单一条件、普通状态分别比较；高 IV 阈值由调用者提供，
不是所有市场都通用的常数。

```bash
python3 examples/crypto/options/crypto_options_panic_regime_response_replay.py \
  --input work/crypto-options-skew-response.jsonl \
  --horizon-records 3 --high-atm-iv 60 --downside-skew-iv 3 \
  --min-observations 5
```

## 证据规则

除非提供方明确给出符号，gamma mass 只是无符号近似。Max pain 是 intrinsic 分布近似，不是结算 PnL 或价格钉住证明。
Put/call OI 不能推导 dealer sign 或交易者归属；IV-RV spread 不是做空波动率建议。示例不建模保证金、delta 对冲、
行权、指派、成交、手续费或执行。

完整命令和出处见 [`README.md`](README.md)。

## 边界

期权系列只读，不下期权单、不对冲、不签名钱包、不管理仓位，也不声称实盘账户 PnL。
