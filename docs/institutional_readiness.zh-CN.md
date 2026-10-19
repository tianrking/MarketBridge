# 机构研究数据服务：使用门槛与验收

日期：2026-10-19。范围：普通 Linux/VPS 上的只读行情与研究数据服务，共享实现继续支持 Windows/macOS。当前适合受控研究和集成验证，尚未达到可承诺无损归档、连续因子生产或机构生产 SLA 的程度。总线合成吞吐不能替代这个结论。

## 本轮实际修复

### 显式 legacy Redis 的失败结果与停机核对（历史阶段）

之前存在可直接影响归档的错误：死信写失败仍清空批次，且提前增加成功计数；停机遗漏本地队列；连接/确认等待缺少超时；主 sink 退出后 intake 子任务可能继续运行。

现在每次连接加批次确认共用 3 秒上限，最多 8 次尝试，退避最多 5 秒。取消中断网络等待，冻结本地接收边界，将当前批次和本地队列中已接收的事件写到 Redis 或死信文件。停机时尚未确认的事件直接走文件恢复路径，不再等待网络恢复。文件完成 `flush` 和 `sync_all` 后才清批次、增加死信计数。

死信失败会保留当前批次直到错误返回，不再把失败计成成功；它并不使内存变成耐久存储。进程失败退出后，未持久化的数据仍需要此前的 journal 或其他权威数据源恢复。订阅 `Lagged` 也会使该 sink 报错，并由主进程监督器失败退出。正常停机收到的业务错误同样能传到进程退出码。

新增指标：

| 指标 | 含义与使用 |
|---|---|
| `redis_accepted_total` | 进入该 sink 本地队列的事件数；不是 source 接收数。 |
| `redis_pending_events` | 已接收但尚未得到 Redis ACK 或完成死信同步的事件数；持续增长需排查。 |
| `redis_xadd_total` | 得到整批 Redis ACK 的逻辑行数；不等于服务端物理新增条数或已落盘条数。 |
| `redis_dead_letter_total` | 完成文件同步的行数；正常停机积压也计入，因此增长需要恢复处理。 |
| `redis_dead_letter_failures_total` | 文件批次写入/同步失败次数；应告警。 |
| `redis_lagged_total` | 已观测到的订阅丢失数；任何非零值都会使归档失败。 |

正常本地排空后满足 `accepted = xadd + dead_letter`，`pending = 0`。这些值也写入退出日志；失败退出时允许 `pending > 0`，不能据此宣称所有数据已经保存。

故障测试使用真实 Redis 客户端连接本地 RESP 端点：卡住确认、写拒绝、ACK 丢失后重连、目录不可写、满队列取消，以及跨三批的 237 条已接收记录停机核对。它验证客户端控制流与文件内容，不替代真实 Redis 的 AOF、复制、故障转移和磁盘验证。

### 派生数据不再隐藏已知订阅缺口

`/v1/market/order-flow`、`/v1/market/order-flow/windows`、`/v1/market/footprint` 和 `/v1/research/symbol-state` 的响应带有 `subscription_quality`；symbol-state 每行也保留该信息，便于归档。具体路由以 [API 注册](../src/api/mod.rs) 为准。

```json
{
  "scope": "process_local_subscription",
  "upstream_completeness": "not_verified",
  "has_known_gap": true,
  "gap_epoch": 1,
  "skipped_events_total": 14,
  "last_gap_detected_at_ms": 123456,
  "recovery_policy": "rebuild_state_required_no_automatic_reset"
}
```

`gap_epoch` 是当前 store 内观测到的非零 lag 通知次数，不是可跨进程比较的行情序号；时间是本机检测时间，不能当成缺失成交的源时间范围。Order-flow 的范围是成交订阅，strategy 的范围是全事件订阅。现有 lag 消息没有丢失标的、domain 或源时间，故保守影响该订阅的全部结果，包括之后首次出现的标的。

`squeeze-radar/v1` 将已知缺口候选标为 `incomplete_data`，强制 `ready_for_trigger=false`；旧 symbol-state 的两个 leg 也变为 `incomplete_data`，分数归零。扫描分数仍可保留作诊断，不能绕过质量状态单独使用它。归档会保留 raw 和 data-quality 中的缺口证据。全新的、满分且价格新鲜的样本在有已知缺口时也必须被拒绝。

缺口不会因为下一条报价、等待一个窗口或清理旧 buckets 而清除。当前没有自动补洞；可以用经过核验的数据集重建状态，或重启后开始新的观测期并重新预热。重启不会补回前一观测期的数据。`has_known_gap=false` 仅表示尚未观测到本地订阅 lag，不证明源序号连续、容量保留完整或所有市场都被订阅。

## 全局保留预算（第一阶段）

新增 `runtime.memory`，在所有标的之间共同限制 order-flow 原始成交条数（默认 250,000）、其 deque 容量与行字符串字节（64 MiB）、raw keys（4,096）、桶数（50,000），以及策略 symbols（2,048）和状态/历史容量字节（64 MiB）。原始成交内存和桶历史分别报告淘汰；策略容量淘汰会使所有相关候选降为 `incomplete_data`。`squeeze-radar/v2` 在 v1 订阅缺口门槛上增加了保留损失门槛。

`/v1/system/memory` 暴露当前预算、保留量和淘汰证据；order-flow/footprint 响应增加 `retention`，symbol-state/扫描增加 `retention_quality`。正常按时间到期与容量强制淘汰分开处理。容量丢失目前仍不自动清除。配置预算必须为正，过小预算可以让当前 symbol 被淘汰，API 保留质量证据而不声称完整。

成交全局到期扫描降为按源时间高水位每秒一次；当前 key 的 500 桶上限和总预算仍逐次检查。回归发现过“同一时间先后更新两个 symbol，第二个多保留一个桶”的边界，已通过独立 key 计数修复，原有每 key 回归继续通过。乱序事件不会回拨清理高水位。

SQLite 写请求队列改为 64 个有界槽位，异步反压，不在 Tokio worker 上阻塞；单请求最多 20,000 行，超限明确拒绝。入口也限制 symbol/trade ID、盘口档数和外部 JSON 的尺寸，避免有界条数乘上无限载荷。这里限制的是输入进入保留链路后的资源，并不声称限制了解码前的所有分配。

这些是组件保留预算，**不是进程 RSS 硬上限**：HashMap/分配器开销、网络缓冲、其他缓存和 API 临时对象需要在完整链路测量中单独核对。第一阶段通过 419 项 Rust 测试、Clippy、locked 构建和 5 项进程测试；宽 symbol、长 trade ID、到期计数和 tiny-budget 测试通过，不能外推为所有 VPS 的内存保证。

## 耐久投递（第二阶段）

已实施稳定事件身份、SQLite 有界 outbox、提交后发布、origin 隔离的明确 Redis ID、内容冲突检查、本地 AOF 确认、ACK 后删除、原身份导出与事务重放。真实 Redis 强杀/重启、ACK 丢失、去重证据删除和 AOF 缺失测试已执行。配置、物理流名迁移和准确的恢复边界见[耐久投递文档](durable_outbox.zh-CN.md)。上一节的广播/死信说明现在仅描述显式 legacy 模式；默认耐久路径从 outbox 直接消费。

## 源连续性与有限恢复（第三阶段）

已实现源观测/消费者进度/存储分开的真实 readiness，源队列丢弃、取消、非法数据和支持协议的原生缺号证据。Binance aggTrade 的原生检查点随 outbox 事务持久化；深度快照不假定相邻 ID +1。扫描模型现为 `squeeze-radar/v3`。见[源契约](source_readiness.zh-CN.md)。

有限成交窗口有独立原生序号和边界证明，HTTP/CLI 重建窗口内数量、金额和从窗口起点锚定的 CVD；不会清除全局 taint 或修复终身 CVD。journal/v2 保留源完整性证据，严格全文件策略回放继续拒绝缺口。详见[窗口恢复](verified_trade_windows.zh-CN.md)。

## 完整链路验收（第四阶段）

可运行的[完整链路工具](pipeline_acceptance.zh-CN.md)启动真实 server、SQLite、Redis AOF 和 HTTP，并中断 Redis、重启 server，逐条核对 CRC、全局身份和成交原生 ID。聚合器也有独立的跨标的 64 MiB / 2048 symbols 默认预算；淘汰会阻止完整性门槛。outbox 批次受 4096 条和 4 MiB 双重限制，避免沿用 legacy 的 100 条批次限制 AOF 确认吞吐。最终实测证据链接随验收完成加入。

## 机构生产前仍需按场景验收

| 范围 | 已落实 | 剩余通过条件 |
|---|---|---|
| 连续性 | 本地质量证据、Binance 成交原生 ID、有限窗口重建 | 逐交易所验证 snapshot/delta、合约乘数、OI/资金费率单位和原始协议样本；不支持协议不能推定连续。 |
| 资源 | 跨标的保留预算、有界写队列、耐久队列磁盘上限、淘汰门槛 | 按目标机流量与深度持续验收 RSS、磁盘、API 和峰值恢复；预算不是进程 RSS 硬限制。 |
| 耐久 | 提交后发布、稳定身份、实际 Redis AOF、ACK 丢失/强杀重启与幂等重放测试 | 目标卷断电、磁盘满、双机故障、复制/故障转移、备份还原与灾备演练，明确 RPO/RTO。 |
| 运维 | 健康/数据就绪分离、版本身份、单调阶段时延、跨平台共享实现与 CI | Linux/Windows/macOS 的目标发行包实机验收；长期（例如 72 小时）采集与目标 SLA。 |
| 研究 | 质量阻止触发、完整预热、同产品价格基线、冻结输入与 API/CLI 核对 | 权威真数据交叉核验、时间留出集、成本敏感性与机构自身模型验收。 |

### 恢复操作

保留实例配置、构建身份、日志、原始 outbox 数据库及一致备份。关闭该实例后使用 `--outbox status/export/replay`；不要修改 origin、序号或删除去重证据以消除告警。默认耐久路径只保证提交边界之后的事件；尚在源/队列中未提交的数据要向权威源追补。共享卷/主机同时损坏不在本地双存储保证内。

显式 legacy 死信仍是追加 JSONL；部分写失败可留下不完整尾行，没有自动死信重放。不要反复 `XADD *` 到权威 stream。默认耐久恢复与它不同，具体操作和限制以[耐久 outbox](durable_outbox.zh-CN.md)为准。局部验证成功仍保留范围外缺口，重建派生状态后重新预热。

## 下一轮部署验收记录

先固定实际场景，再决定容量：机器与配额、源/标的清单、domain、盘口深度、均值及峰值事件率、API 并发、磁盘预算、允许的新鲜度/缺口/恢复目标。当前没有经过实测的通用 VPS 规格或机构 SLA。

在目标机器至少进行一次约定时长的持续采集和故障演练；正式发布清单建议 72 小时观察。保留提交、配置、测试时间、原始测量和失败记录。验收应包含峰值后排空、慢订阅者、断网、Redis ACK 丢失、磁盘写失败及重启，逐项核对质量标记、记录数量和退出状态。多平台通过同一套功能测试以后，再分别建立性能基线。

本轮故障测试命令：

```bash
cargo test --locked redis_sink
cargo test --locked subscriber_loss
cargo test --locked known_subscriber_gap
cargo test --locked lifecycle_tests
```

历史阶段 Linux 本地结果：414 项 Rust 测试、13 项 Python SDK 测试、529 项 Python 示例测试、5 项真实进程测试通过；fmt、Clippy 和 locked 构建通过。实际 HTTP 验证覆盖 9 个归档研究模型、同步/异步 SDK、API/CLI 一致性、SQLite 完整性和重启后归档恢复。[验证记录](validation/2026-10-19-institutional.json)保留对应源码提交、二进制 SHA-256、命令和本地日志摘要哈希；它不是远程 CI 证明，二进制也未内嵌经过认证的构建身份。

没有把本地 RESP 测试描述为真实交易所、真实 Redis 集群或多日生产验证。普通 Linux/VPS 是默认方向，当前这些门槛也不会由切换 PREEMPT_RT 自动解决。
