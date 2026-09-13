# MarketBridge 使用手册

本目录说明如何使用当前源码中的功能，不把路线图视为现成功能。MarketBridge
提供市场数据与策略研究能力，不签名、不管理真实交易账户、不下单。

0. [从启动到第一次实验](00-quickstart.md)
1. [研究工作区、资产目录与实验归档](01-workspace.md)
2. [各类研究模型与单位约定](02-models.md)
3. [多市场数据接入与真实性](03-sources.md)
4. [自动扫描、告警和热重载](04-scanner.md)
5. [录制、完整文件回放和数据集](05-history.md)
6. [组合库存、部分成交和退出](06-portfolio.md)
7. [公告事件与窗口研究](07-events.md)
8. [Python SDK 和研究工作台](08-sdk-workbench.md)
9. [运行、备份、排障和发布验收](09-operations.md)
10. [扩展市场、策略和研究业务](10-extending.md)
11. [用 MarketBridge 寻找永续合约逼空结构](11-squeeze-radar.md)

现有实时采集配置、成本曲线、扫描、回放和模拟账本的完整字段说明另见
[研究 API](../research-api.md)。开发验证记录见[开发日志](../development-log.md)。

运行 API 前从项目根目录设置 `MARKETBRIDGE_CONFIG=config.research.yaml`。
这是不联网的本地研究模式；`config.research-live.yaml` 是可选公共源观察模式。
如设置 `MARKETBRIDGE_API_KEY`，所有请求加入 `x-api-key` 请求头。

第一次使用按 00 → 03 → 01 → 02 阅读。关注持续观察看 04/07；关注历史验证看 05/06；
集成自己的系统看 08，部署前务必看 09。实际覆盖与验收状态以开发日志和功能清单为准。
