# 自动补采与核验后恢复窗口

Binance spot、USD-M 的 aggTrade 原生 ID 出现缺号后，系统自动建立耐久恢复任务，通过官方 REST aggTrades 分页补采，核验闭合窗口后才公开恢复值。其他交易所没有被默认当作可连续认证的数据源。此功能恢复成交窗口；当前 L2 的重新同步见 [订单簿重建](reconstructed_order_books.zh-CN.md)。交易所不提供历史逐条深度时，无法补造断线期间的订单簿或 OFI。

## 工作流程

1. 正常入站发现相邻 aggTrade ID 不连续，冻结缺口前后两条真实成交的 ID、源时间、价格、数量和主动方向。恢复任务、原生 checkpoint 与启用 outbox 时的事件在同一 SQLite 事务提交；事务失败不推进耐久 checkpoint。
2. 将受影响范围按默认一分钟向两端扩展；尚未闭合的窗口进入 await_boundary。REST 请求按 fromId 包含式分页，向前查到真实窗口前边界，再连续读取到窗口后边界。每页及跨页检查 ID 连续、时间不倒退、有效数量和方向，且冻结的两条实盘成交必须与 REST 一致。
3. 分页原始记录有界持久化，保留每条 REST JSON 对象及原始价格、数量字符串；核验时重新解析原生记录，检查规范化数值一致性。只有两个边界均存在、窗口连续且累计运算有效，任务才从 running 封存为 verified。原始记录绑定 job ID、原生 ID 和 CRC；报告也有 CRC。查询会重新读取并核验原始记录，损坏返回 503，部分补采不会返回恢复数值。CRC 是损坏检测，不是交易所签名。
4. 网络错误或历史暂未可用按退避重试；429/418 尊重 Retry-After，并与 Binance 深度快照共享请求预算。协议冲突、无历史、超限制进入 blocked；重启将 running 重置为 pending，重新补采未完成任务，已核验任务保留。

补采历史只进入专用恢复证据库，不插入实时累计 CVD、实时分发流或 Redis 成交流。有限窗口以窗口起点为累计基准，global_quality_cleared=false、lifetime_cvd_repaired=false；发现过的数据丢失不会因一段补齐而自动清零。

## 使用

默认 runtime.recovery.enabled=true，但只在配置 Binance trade 采集器时启动。启用耐久 outbox 时共用其 SQLite 数据库；否则使用 runtime.recovery.path 的紧凑数据库，仅存原生 checkpoint、恢复任务和有限证据，不暗中无限归档实时事件。MARKETBRIDGE_RECOVERY_DB 可覆盖独立恢复库路径。

- GET /v1/system/recovery：任务、状态、尝试次数、范围、预算、延期流数、淘汰计数。verified 表示曾完成封存；原始行的完整检查发生在窗口查询。
- GET /v1/market/order-flow/recovered-window?exchange=binance&market=spot&symbol=BTCUSDT&start_ms=...&end_ms=...：查询已封存范围或其可认证子窗口。200 才有通过核验的结果；400 请求不合法，422 没有可认证证据，503 证据损坏或存储错误。
- `market-bridge --recovery status data/trade-recovery.sqlite`：只读状态，可在服务运行时执行；启用 outbox 时使用该 outbox 路径。
- `market-bridge --recovery retry data/trade-recovery.sqlite JOB_ID`：停服后重新排队未核验任务。需要数据库独占 lease；拒绝修改已封存证据。可先调整预算/网络设置再重启。没有向公网暴露任意 URL 或重试写入接口。

## 明确的资源限制

默认最多 256 个任务、4096 个延期流、每任务 100000 行、128 MiB 原始证据预算、独立数据库 256 MiB、单窗口最多 15 分钟、每次尝试最多 256 次请求、每请求 10 秒、最多 1000 次尝试。Binance REST 每页最多 1000 行，响应上限 2 MiB；权重保守按每请求 20 计费。数据库页上限不包含 WAL、lease 或外部导出文件，证据字节是保守计量，不是 RSS。

任务满时淘汰最旧终态任务和其证据，并公开计数；全是活动任务时，将同一流的新缺口合并为耐久延期债务。合并范围可能包含已观测成交，因此 candidate_from/to_sequence 不声称是精确缺失条数。延期流也超预算时事务失败，不能默默丢弃恢复义务。证据预算不足会先淘汰其他终态任务；仍无法容纳则阻止核验。需要长期审计时，应将已核验结果及数据库备份送入机构自有归档，不能把滚动缓存当无限历史库。

分钟内成交超过行数限制、缺口跨越上限、交易所历史不可取、交易所返回缺号或冻结成交冲突时，系统会明确拒绝认证。提高限额前应实测 VPS 磁盘与请求预算；代码支持不代表任意市场、任意故障都一定可以补回。

## 可重复验收

构建后执行：

```sh
python -B scripts/accept_native_protocols.py --binary target/debug/market-bridge --output /tmp/mb-native-check
# 可加入 --redis-server /path/to/redis-server，验证真实 Redis AOF 和同库 outbox。
```

工具启动独立真实服务进程及仅回环 HTTP/WebSocket 协议对端，制造成交 10→13、深度 11→13、第一次 REST 429、分页、重启和证据损坏。核验 1501 条窗口成交、前后原生边界、旧订单簿失效与新会话重建，以及补采未混入实时累计和 Redis 成交流。原始 JSON 报告、请求记录、二进制及源码 SHA256 留在指定目录。这是生产协议代码的本地故障验收，不是公网交易所或长期 VPS 运行证明。
