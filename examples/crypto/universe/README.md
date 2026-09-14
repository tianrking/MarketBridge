# Universe and cross-asset / 资产宇宙与跨资产研究

> **Language / 语言:** [English guide](README.en.md) · [简体中文指南](README.zh-CN.md)

## Scope / 研究范围

研究 point-in-time 候选排名、breadth、跨资产 lead-lag/相关性、pairs、回撤恢复和市场状态；
候选列表不是资金分配，也不构成官方指数或组合。

| Theme / 主题 | Typical entrypoints / 典型入口 |
|---|---|
| Opportunity universe | `crypto_universe_opportunity_*`, `crypto_universe_delist_risk_monitor.py` |
| Cross-asset | `crypto_cross_asset_*`, `crypto_pairs_mean_reversion_replay.py` |
| Breadth/regime | `crypto_altcoin_breadth_replay.py`, `crypto_global_market_regime_*` |
| Risk response | `crypto_drawdown_recovery_*`, `crypto_trend_template_*` |

## Quickstart / 快速开始

```bash
python3 examples/crypto/universe/crypto_universe_opportunity_scan.py \
  --exchange binance --market perp --interval 5m --min-score 2
```

完整参数、精确时间戳对齐、成本敏感性和来源说明见双语指南。缺失成分、退市身份、宇宙定义和
等权假设都必须显式记录。

## Boundary / 边界

不分配资金、不自动再平衡、不声称 Sharpe 或因果关系。Rust 只提供标准化数据和质量元数据，
Python 负责研究假设与回放。

## English

The maintained English guide is [README.en.md](README.en.md).

## 中文

维护中的中文指南是 [README.zh-CN.md](README.zh-CN.md)。

## Commands / 命令

运行入口统一使用 `python3 examples/crypto/universe/<entrypoint>.py`；参数、出处和限制见上述双语指南。
