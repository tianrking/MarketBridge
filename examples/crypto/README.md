# Crypto strategy library / 加密策略案例库

This directory is the Python-first crypto research surface. MarketBridge Rust
collects, normalizes, caches and serves data; these examples only observe,
replay and score hypotheses. They never place orders, sign wallets or call
private trading APIs.

这里是 Python 优先的加密研究入口。MarketBridge 的 Rust 层负责采集、标准化、缓存和
HTTP 接口；目录里的案例只观察、回放和评分假设，不下单、不签名钱包、不调用私有交易接口。

## Categories / 分类

| Directory / 目录 | Focus / 主题 | Start here / 入口 |
|---|---|---|
| [`carry/`](carry/README.md) | Basis, funding, OI impulse/positioning, cross-sectional funding, cross-venue gaps, order-book and triangular quote edges / 基差、资金费率、OI 冲击/持仓上下文、横截面资金费率、跨交易所价差、盘口与三角报价 edge | `basis_carry_monitor.py`, funding and OI replays |
| [`microstructure/`](microstructure/README.md) | Squeeze, exhaustion, Bollinger/volatility breakouts, CVD divergence, quarter-hour/session/LVN/footprint confluence, aggregate derivatives sentiment, liquidation and liquidity stress / 逼空、衰竭、Bollinger/波动率突破、CVD 背离、季度小时/时段/LVN/footprint 共振、衍生品聚合情绪、清算与流动性压力 | `short_squeeze_monitor.py`, `crypto_bollinger_squeeze_replay.py` |
| [`options/`](options/README.md) | IV, skew, VRP, gamma, bull-call spreads and volatility / IV、偏斜、VRP、Gamma、牛市看涨价差与波动率 | options monitors, recorders and replays |
| [`defi/`](defi/README.md) | DEX pool flow, liquidity, turnover and stablecoin depeg risk / DEX 池流量、流动性、换手与稳定币脱锚风险 | `crypto_defi_pool_flow_monitor.py`, stablecoin replay |
| [`onchain/`](onchain/README.md) | Large-transfer bursts and bounded price-response replay / 大额链上转账 burst 与有限价格响应回放 | `crypto_onchain_transfer_burst_replay.py` |
| [`universe/`](universe/README.md) | Cross-asset ranking, pair mean reversion, stale-quote risk, market regime, volatility adjustment, parameter sweeps, holdouts and opportunity discovery / 跨资产排名、配对均值回归、过期报价风险、市场状态、波动率调整、参数扫描、样本外切分与机会发现 | momentum, pair, risk, regime, sweep, walk-forward and universe scan |
| [`macro/`](macro/README.md) | DXY, VIX, US10Y and crypto funding context / DXY、VIX、US10Y 与 crypto 资金费率上下文 | `crypto_macro_context_monitor.py` |
| [`sentiment/`](sentiment/README.md) | Fear & Greed extremes, CryptoPanic news attention and fixed-horizon response replays / Fear & Greed 极值、CryptoPanic 新闻注意力与固定窗口响应回放 | sentiment monitors and recorders |

The categorised entrypoints are thin Python launchers around shared Python
implementations. Some older snapshot observers also route through the shared
`../python_strategy_runner.py`; recorder/replay cases keep their own explicit
parameters and JSONL contract. This avoids drifting copies while preserving
simple commands for researchers.

分类入口都是调用共享 Python 实现的轻量启动器；部分较早的快照观察器仍经由共享
`../python_strategy_runner.py`，而 recorder/replay 案例保留自己的参数和 JSONL 契约。
这样可以避免多份实现逐渐分叉，同时让研究者用直观目录和命令运行。

## Common setup / 通用启动

```bash
MARKETBRIDGE_CONFIG=./config.squeeze-radar.example.yaml cargo run
python3 examples/crypto/microstructure/short_squeeze_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
```

The server command is infrastructure startup, not a strategy implementation.
All strategy files in this library are Python.

服务端命令只是启动数据基础设施，不是策略实现；本库所有策略文件均为 Python。
