# Options and volatility / 期权与波动率

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

覆盖 IV skew、近远端期限结构、gamma mass、VRP、DVOL、put/call OI、max pain 和 bull-call
spread 几何。所有结果都是 surface/quote response study，不虚构 dealer sign、期权 PnL 或对冲。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Skew/term structure | `crypto_options_skew_*`, `crypto_options_term_structure_*` |
| Gamma/max pain | `crypto_options_gamma_*`, `crypto_options_max_pain_*` |
| IV/RV | `crypto_options_vrp_*`, `crypto_deribit_*` |
| OI/spreads | `crypto_options_put_call_oi_*`, `crypto_options_bull_call_spread_*` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/options/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30
```

先检查 expiry、moneyness、greeks 和 coverage，再使用 recorder/replay。完整命令与出处见双语指南。

## Boundary / 边界

gamma mass 默认是无符号近似，max pain 是 intrinsic proxy；不建模保证金、delta 对冲、行权、
指派、成交或手续费，不执行期权交易。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/options/<entrypoint>.py`；参数、出处和限制见上述双语指南。
