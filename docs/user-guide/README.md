# MarketBridge 使用手册

本目录说明如何使用当前源码中的功能，不把路线图视为现成功能。MarketBridge
提供市场数据与策略研究能力，不签名、不管理真实交易账户、不下单。

1. [研究工作区、资产目录与实验归档](01-workspace.md)
2. [各类研究模型与单位约定](02-models.md)

现有实时采集配置、成本曲线、扫描、回放和模拟账本的完整字段说明另见
[研究 API](../research-api.md)。开发验证记录见[开发日志](../development-log.md)。

运行 API 前从项目根目录设置 `MARKETBRIDGE_CONFIG=config.research.yaml`。
这是不联网的本地研究模式；`config.research-live.yaml` 是可选公共源观察模式。
如设置 `MARKETBRIDGE_API_KEY`，所有请求加入 `x-api-key` 请求头。

本手册会随实际实现扩充。尚未编写或尚未通过验收的模块不会标成已完成。
