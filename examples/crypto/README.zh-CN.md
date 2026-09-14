# 加密研究系列

MarketBridge 以 Python 承载加密策略研究，Rust 负责连接器、标准化、历史、缓存以及
HTTP/WebSocket API。下面各系列按验证假设所需的证据分类，全部只读。

| 系列 | 主要证据 | 说明 |
|---|---|---|
| 资金费率 | basis、funding、跨交易所盘口、价格差 | [`carry/README.zh-CN.md`](carry/README.zh-CN.md) |
| DeFi | 池状态、稳定币、路由冲击、收益上下文 | [`defi/README.zh-CN.md`](defi/README.zh-CN.md) |
| 宏观 | DXY/VIX/US10Y、ETF 流量、聚合状态 | [`macro/README.zh-CN.md`](macro/README.zh-CN.md) |
| 微结构 | flow、OI、深度、清算、波动率 | [`microstructure/README.zh-CN.md`](microstructure/README.zh-CN.md) |
| 链上 | 转账、mempool、矿工 | [`onchain/README.zh-CN.md`](onchain/README.zh-CN.md) |
| 期权 | skew、期限结构、gamma、VRP、max pain | [`options/README.zh-CN.md`](options/README.zh-CN.md) |
| 情绪 | 恐惧贪婪、新闻、社交指标 | [`sentiment/README.zh-CN.md`](sentiment/README.zh-CN.md) |
| 资产宇宙 | breadth、排名、配对、市场状态 | [`universe/README.zh-CN.md`](universe/README.zh-CN.md) |

共享 [`strategy/README.zh-CN.md`](strategy/README.zh-CN.md) 运行器提供常用只读案例的
Python CLI。它只是便捷层；每个假设和限制仍以各分类指南为准。

## 文件职责

- `*_monitor.py` 读取当前 MarketBridge 响应并输出结构化证据。
- `*_recorder.py` 把重复观测冻结为追加式 JSONL。
- `*_replay.py` 读取归档或有限历史并报告分布。
- `*_response_recorder.py` / `*_response_replay.py` 加入同步 BTC 报价，测试后续响应，
  而不是宣称一次性信号。

先运行 monitor，再运行 recorder；解释 replay 前检查提供方、时间戳、覆盖和缺失字段。
简洁跨系列地图见 [`../README.md`](../README.md)，每个系列指南维护自己的案例目录；各目录的英文页统一命名为 `README.en.md`。

## 共同边界

任何加密示例都不下单、不签名钱包、不转移资金、不管理仓位、不声称实盘 PnL。手续费、
借贷、资金费率、延迟、滑点、流动性、排队和成交，除非案例明确写出纸面假设，否则都保留为研究缺口。
