# 03 · 多市场数据接入与真实性

## 三种不同的“支持”

1. 连接器有代码：看 [feature_inventory](../feature_inventory.md)。
2. 当前配置已启用且收到数据：看 `/v1/catalog/sources`、`/v1/catalog/health` 和实际行情时间。
3. 可用于某个模型：还要满足资产关系、真实买卖边、单位、时间偏差、成本与深度要求。

只有第一项不代表第二项或第三项。`source-roadmap` 是候选目录，不是已经接通的服务。

## 选择数据源

| 需求 | 配置/查询入口 | 注意事项 |
|---|---|---|
| CEX 现货、永续 | `exchanges`、`symbols`、`perp_symbols`；market quotes/books/funding | 不同产品使用本地符号；期货乘数不能当币数 |
| DEX / 黄金 token / 链上包装资产 | `defi`；market quotes | 单价或人工扩出来的 bid/ask 只作参考；验证链、地址、发行与赎回条件 |
| 股票、宏观、商品参考 | `tradfi`；quotes/external signals | 延迟、交易时段、单位、数据授权与合约期限分别验证 |
| 期权 / 预测市场 | options、prediction 专用端点 | 不接入同资产现货公式硬算利润 |
| 新闻、情绪、自定义 HTTP | `sentiment`、`aggregates.custom_apis`；external signals | 使用各源原有映射配置；不是任意网页爬虫 |
| 历史 K 线 | `/v1/history/candles`、storage manifest | K 线不能重建盘中双边订单簿或真实成交队列 |

具体字段、API Key 环境变量和示例见[数据源手册](../data_sources.md)、
[完整接口说明](../data_interfaces.md)、[查询示例](../query_examples.md)。
不要在不知道源支持的情况下猜 URL、合约地址或 ticker。

## 推荐的接入流程

从单个来源、少量标的开始 → 检查原始源与统一输出的单位 → 观察重连和 stale 状态 →
录制有限时间 → 建立资产关系版本 → 选择适用模型。扩大订阅前评估限额、响应大小和磁盘。

对 PAXG / XAUt / 黄金期货或 ETF，可以登记 `hedge`、`convertible` 或 `correlated`。
`same_asset` 要求更严格。同名、同样跟踪黄金，不代表同一可赎回权利，也不能默认原子交换。
系统不会因为两个价格差大就把它们升级为可成交套利。

## 时间和缺失值

保留 source time、系统 received/known time 和研究 as-of time。来源可能只提供接收时间精度，
需要在研究说明中保留该限制。不要把当前价格填进过去的样本。

未知费用用 JSON `null`，不是 `0`。未知深度不能用无限数量代替。
盘面参考价、延迟指标、模拟 bid/ask 和观测订单簿不是相同证据等级。
目前实时扫描只将已审查的 Binance spot depth20 / OKX spot books5 缓存提升为观测簿；
其他来源可以覆盖数据与研究，但不会自动得到同样的可成交证据等级。

公共访问不是商用转售许可。发布收费 API 前逐源核实服务条款、缓存和再分发权限；
不要靠轮换 IP 或账号绕过平台限额。

## 公共接口配额

对 `aggregates.custom_apis`，可以在同一层配置命名的共享窗口，再由每一条来源
声明它要消耗的权重：

```yaml
aggregates:
  provider_quotas:
    - name: public-metals
      max_requests: 30
      window_secs: 60
  custom_apis:
    - enabled: true
      name: xau-reference
      url: "https://provider.example/api/xau"
      metric: price
      value_path: price
      quota_group: public-metals
      quota_weight: 1
```

同组请求在发出前共同扣减额度；额度耗尽会等到窗口结束，而不是并发冲击上游。
这只是本机的保守保护，不替代数据商按账号、IP、套餐或端点制定的规则。配置中
引用的组必须存在，`quota_weight` 必须为正且不超过 `max_requests`。

运行后用以下只读接口核对本机窗口的已用与剩余额度；没有已初始化的命名配额时，
返回空数组：

```powershell
Invoke-RestMethod 'http://127.0.0.1:8080/v1/system/provider-quotas'
```
