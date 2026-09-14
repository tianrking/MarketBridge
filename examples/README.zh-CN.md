# MarketBridge 示例库

> **范围：** 以 Python 为主、只读的市场研究案例。Rust 仍是数据层；示例只负责
> 请求数据、记录快照、回放假设和报告证据。

这是示例库的简体中文入口。完整脚本目录仍保留在 [`README.md`](README.md)；下面每个
系列都有对应的英文说明 `README.en.md`。

## 按研究主题进入

| 系列 | 适用问题 | 说明 |
|---|---|---|
| 加密资金费率 | basis、资金费率、跨交易所和三角价差 | [`crypto/carry/`](crypto/carry/README.zh-CN.md) |
| 加密 DeFi | 池流量、稳定币、路由冲击和流动性上下文 | [`crypto/defi/`](crypto/defi/README.zh-CN.md) |
| 加密宏观 | DXY/VIX/US10Y、ETF 流量、稳定币流动性冲击和市场状态 | [`crypto/macro/`](crypto/macro/README.zh-CN.md) |
| 加密微结构 | 订单流、OI、清算、深度和响应研究 | [`crypto/microstructure/`](crypto/microstructure/README.zh-CN.md) |
| 加密链上 | 转账、mempool 和矿工压力 | [`crypto/onchain/`](crypto/onchain/README.zh-CN.md) |
| 加密期权 | IV 曲面、skew、gamma、VRP 和 max-pain 近似 | [`crypto/options/`](crypto/options/README.zh-CN.md) |
| 加密情绪 | 恐惧贪婪、新闻关注度和社交指标 | [`crypto/sentiment/`](crypto/sentiment/README.zh-CN.md) |
| 加密资产宇宙 | breadth、跨资产排名、回撤响应和相对价值 | [`crypto/universe/`](crypto/universe/README.zh-CN.md) |
| 预测市场 | 公开成交流、校准和结算回放 | [`prediction/`](prediction/README.zh-CN.md) |
| 天气 | 确定性观测和市场校准输入 | [`weather/`](weather/README.zh-CN.md) |

共享的 [`crypto/strategy/`](crypto/strategy/README.zh-CN.md) 运行器为 Python 研究者提供
统一入口；它不会替代各分类指南，也不会引入第二套数据契约。

## 统一研究流程

1. 用只读研究配置启动 MarketBridge。
2. 先运行一个 `*_monitor.py`，检查实时字段和缺失数据。
3. 用 `*_recorder.py` 写入追加式 JSONL 快照。
4. 用 `*_replay.py` 指定前瞻窗口、最小样本和纸面成本。
5. `observe_only`、覆盖不足、提供方语义和小样本都必须原样保留，不能擅自填零。

```bash
MARKETBRIDGE_CONFIG=config.research.yaml cargo run
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance
```

大多数 recorder 默认写入 `work/*.jsonl`。这些文件只是研究材料，不包含私钥、签名
交易或下单指令。

## 不可突破的边界

示例绝不下单、撤单、改单、签名钱包、转移资金、管理仓位，也不声称实盘账户 PnL。
每个案例都必须在代码和输出中明确提供方、时间戳、覆盖范围、前视规则、成本假设以及
失效条件/限制。

## 验证

在仓库根目录运行：

```bash
PYTHONPATH=examples/crypto/options:examples/crypto/defi:examples/crypto/microstructure:examples/crypto/onchain:examples/crypto/carry:examples/crypto/macro:examples/crypto/universe:examples/crypto/sentiment:examples/crypto/strategy:examples/prediction:examples/weather \
  python3 -m unittest discover -s examples/tests -p 'test_*.py'
python3 -m compileall -q examples
```

策略接入验收清单见 [`docs/user-guide/12-strategy-intake.md`](../docs/user-guide/12-strategy-intake.md)，
共享运行器说明见 [`crypto/strategy/README.zh-CN.md`](crypto/strategy/README.zh-CN.md)；
完整案例、出处和限制见 [`README.md`](README.md)。
