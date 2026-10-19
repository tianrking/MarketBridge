# 订单簿重建与使用契约

Binance 现货/USD-M、OKX books、Bybit spot/linear 增量深度 的深度采集已接入有界 L2 状态机。共享 Rust 实现不需要实时内核；其他连接器继续可用，但没有重建证据的盘口明确为未验证，不能按交易所名称自动升级为可信深度。

## 协议规则

| 协议 | 同步和恢复 | 检查与覆盖 |
|---|---|---|
| Binance spot | 先连 diff depth，缓冲期间请求 REST 1000 档快照，再找到覆盖下一 ID 的 U/u 增量；失步重新引导 | 相邻覆盖、源时间、价位/数量；不是将 depth20 快照冒充完整簿。 |
| Binance USD-M | 首增量满足 U≤快照 ID≤u，随后 pu 必须等于上一 u | 原生 ID 可以批量跳跃；不错误要求 u 每次 +1。 |
| OKX | books 400 档 snapshot，随后 update 的 prevSeqId 必须匹配；新 snapshot 重置 | 保留原始十进制字符串，服务端提供 checksum 时验证交错前 25 档 signed CRC32。没有 checksum 时显式标注未提供；匹配 prevSeqId 的序号回退可按协议重置。 |
| Bybit | orderbook.{depth} snapshot 后合并 delta；新 snapshot/u=1 重置，非法增量重订阅 | u 和 cross-seq 检查倒退/冲突。协议未承诺逐条 +1，也不提供同等 checksum；质量字段明确没有原生缺号认证。 |

每标的独立重连，连接关闭、解析失败、校验失败、超预算或任务取消后，旧连接 lease 立即失效。其他健康标的不随一个标的重连。Binance 快照有已知价格范围边界；只输出该范围内可确认的档位，保留深度裁剪只会收紧范围，耗尽后必须重取快照。任何协议均不声称覆盖无限深度。

内部价格使用检查溢出的 u128 固定 18 位十进制键，保留原始价格/数量文本；不使用 f64 查找/删除价位。零数量删除，负值、重复同价、非有限值、交叉/锁定盘口或无法用现有 f64 输出区分的价位拒绝发布。当前接口的 f64 字段是兼容输出，精确证据在 book_quality.exact_bids/exact_asks 中。

OKX 永续数量为合约张数。每次连接先读取 public instruments，只有 ctType=linear、ctValCcy 为基础币、USDT 结算且 ctMult=1 的已验证合约，才按 ctVal 转为基础币数量。校验使用原始张数文本，输出保留 native_quantity_unit 和 base_quantity_multiplier。逆向、未知乘数或缺元数据明确拒绝，不能生成错误的 USDT 深度。

## 消费门槛

当前簿和缓存视图分别检查单调时钟新鲜度，不能用不断到达的新帧替长期未消费的旧快照续期。

GET /v1/system/order-books 返回逐标的 session、同步状态、新鲜度、重同步次数与全局资源。GET /v1/market/order-books 保留快照，并动态检查该快照的连接是否仍有效；book_quality 缺失表示未验证，synchronized=false 表示不能当当前可信盘口。/ready 也要求受管订单簿同步且新鲜。

自动研究与聚合器取消了 Binance/OKX 名称白名单，要求实际连接证据。断线后的旧缓存立即停止合格。策略 OFI 在每次新 snapshot generation 清空历史，重新观察完整 60 秒；squeeze-radar/v4 展示同步/窗口状态，在重建完成前禁止触发。恢复当前盘口不等于补回断线期间的历史订单簿或终身累计因子。

成功引导和重同步失败生成 order_book_integrity 外部事件；正常入站、journal、耐久 outbox 与 Redis 会保留这些事件，盘口自身也携带原生快照/增量、checksum、session 和本地证据 CRC。取消/强杀仍以 lease 失效和下次新 session 处理，不承诺崩溃瞬间的每个诊断事件都已提交。CRC 检测损坏，不是交易所签名。

## 资源与验收

runtime.order_books 设置符号数、每簿档数、发布档数、全局状态/引导缓冲字节及单簿缓冲；引导缓冲按解析后 JSON 自有分配计量，聚合器也计入精确档位证据字符串。字节是自有结构的保守估算，不是 RSS 硬上限。默认分别 64 MiB 状态与 64 MiB 引导缓冲，发布最多 200 档/侧。Binance spot 1000 档请求按 50 weight、USD-M 按 20 weight 共享限速；429/418 尊重 Retry-After 并暂停已排期请求，重连使用退避。

本地测试使用真实 TCP/WebSocket/HTTP 和协议形状固定的帧，覆盖延迟快照、引导缓冲、丢号、坏 checksum、重启、缓存 lease 失效与数量单位。参考 [Binance 官方说明](https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md#how-to-manage-a-local-order-book-correctly)、[Bybit 官方说明](https://github.com/bybit-exchange/docs/blob/master/docs/v5/websocket/public/orderbook.mdx) 与 [OKX 官方说明](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-order-book-channel)。当前环境未开放交易所域名；没有声称完成公网实机订阅或全部协议/逆向产品验收。

Bybit 默认订阅 1000 档（200 ms），可将 bybit_depth 显式设为 50（20 ms）或 200（100 ms）；发布档数由 publish_levels 独立限制。Bybit 官方说明该数据不包含 RPI 订单，不能称作包含隐藏/RPI 流动性的全市场视图。

自动补齐成交缺口与核验窗口的工作流程、预算和操作命令见 [自动恢复](automatic_trade_recovery.zh-CN.md)。
