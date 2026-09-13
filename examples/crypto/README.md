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
| [`carry/`](carry/README.md) | Basis, funding and convergence / 基差、资金费率与收敛 | `basis_carry_monitor.py`, funding replays |
| [`microstructure/`](microstructure/README.md) | Squeeze, exhaustion, order flow, liquidation and liquidity stress / 逼空、衰竭、订单流、清算与流动性压力 | `short_squeeze_monitor.py`, `liquidity_stress_monitor.py` |
| [`options/`](options/README.md) | IV, skew, VRP, gamma and volatility / IV、偏斜、VRP、Gamma 与波动率 | options monitors and recorders |
| [`universe/`](universe/README.md) | Cross-asset ranking and opportunity discovery / 跨资产排名与机会发现 | momentum replay and universe scan |

The categorised entrypoints are thin Python launchers around the shared
`../python_strategy_runner.py`; this keeps one implementation and avoids
drifting copies while preserving simple commands for researchers.

分类入口是调用共享 `../python_strategy_runner.py` 的轻量 Python 启动器，避免多份实现
逐渐分叉，同时让研究者可以用直观的目录和命令运行。

## Common setup / 通用启动

```bash
MARKETBRIDGE_CONFIG=./config.squeeze-radar.example.yaml cargo run
python3 examples/crypto/microstructure/short_squeeze_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
```

The server command is infrastructure startup, not a strategy implementation.
All strategy files in this library are Python.

服务端命令只是启动数据基础设施，不是策略实现；本库所有策略文件均为 Python。
