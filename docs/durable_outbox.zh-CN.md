# 耐久 outbox、事件身份与 Redis 重放

## 使用与迁移

配置 `runtime.redis_url` 后，默认采用耐久路径：SourceContext → router → SQLite outbox 提交 → journal/总线/派生计算；独立 worker 从 outbox 投递 Redis。归档不再依赖可能丢失的广播订阅。`runtime.outbox.enabled: true` 也可以在未启用 Redis 时保留待导出的事件，达到预算后会明确失败停止。

```yaml
runtime:
  redis_url: redis://127.0.0.1:6379
  redis_stream_prefix: ticks
  outbox:
    path: data/events.outbox.sqlite
    max_pending_bytes: 268435456
    max_database_bytes: 536870912
    require_redis_aof: true
    legacy_redis: false
```

路径可以用 `MARKETBRIDGE_OUTBOX_DB` 覆盖。每个实例必须使用独立数据库；同一数据库的进程租约会阻止第二个实例或离线工具同时获得投递所有权。备份时须停止该实例或使用经过验证的 SQLite 一致性备份方式，不能只复制正在写入的主文件而遗漏 WAL。

Redis 耐久路径需要 Redis 7.2+、AOF 和 `EVAL/XRANGE/XADD/SET/WAITAOF/PING` 权限。默认在写入后等待 `WAITAOF 1 0 1000` 的本地 fsync 确认；不足时保留 outbox、重试并报告未就绪。设 `require_redis_aof: false` 表示只要求普通服务端 ACK，不能据此宣称进程崩溃后的 Redis 数据耐久性。这里只验证单 Redis 实例的本地 AOF，未实现或证明副本故障转移、跨机房容灾或 Redis Cluster 客户端路由。

**流名发生明确变化**：逻辑 `ticks:trade:binance:BTCUSDT` 对应物理 `ticks:trade:binance:BTCUSDT:origin:<32位origin>`。消费者需要发现并消费每个 origin 的流；每条记录包含 `event_id`、`event_crc32`、`received_at_ms` 及原有 source/domain/symbol/ts/payload 字段。stream ID 是 `<ingest-sequence>-0`，并非交易所时间戳，不可用来推导跨 origin 的时间顺序。

必须保持旧物理流名时可显式设置 `runtime.outbox.legacy_redis: true` 且 `enabled: false`。这会使用旧广播/JSONL 路径，保留已修复的失败计数与本地排空，但不获得 durable outbox 和幂等恢复保证。旧死信文件不是新 outbox 导出格式，不能直接冒充受检重放文件。

## 保证的边界

每个规范化事件拥有 `event_identity={origin,sequence}`。普通模式使用当前进程的随机 origin 和单调序号；耐久模式使用 SQLite 中保存的随机 origin 和 `AUTOINCREMENT` 序号。进程重启不会复用已确认身份；普通模式不提供崩溃恢复。总线 lazy JSON 和 journal 使用同一身份。此身份标识一次被接收的规范化事件，不自动证明所有交易所原生 ID 等价事件都已去重。

Router 最多将 64 条入站事件放入同一 SQLite `WAL + synchronous=FULL` 事务，提交成功后才发布。逻辑 pending 字节和数据库 page 数分别有上限；事务超额或磁盘写失败整批回滚，不发布该批数据，并由监督器失败退出。主数据库上限不包含 WAL 峰值、导出文件或其他存储，部署须为这些开销预留独立磁盘预算。

Redis 写入脚本先查指定 ID，存在时核对身份、payload 和记录 CRC，完全一致才视为重放；不同内容报冲突。不存在时使用明确 ID 插入。物理流按持久 origin 隔离，因此多个实例不会竞争同一个序号空间。若先前记录被 `XDEL`/trim 删除但 stream 高水位仍存在，重放会报错保留 outbox，不会只凭高水位跳过缺失记录。

重复脚本本身可能不产生新写入，所以 fsync 前会在同一连接写一个 origin 级 barrier。这样 ACK 丢失后新连接的 `WAITAOF` 覆盖原来 XADD 之后的全局 AOF 位置，避免只等待新连接的空写历史。只有整批脚本成功、所需 AOF 确认成功后，才事务删除对应 outbox 行。连接断开、部分成功或本地 ACK 删除失败时，重放使用相同身份。

完整性限于已提交 outbox 的规范化事件。解码之前、入口主动丢弃或没有序号证明的上游数据不能据此称为完整；SQLite/文件系统/卷/硬件的耐久策略仍需要部署验收。CRC 用于损坏检测，不用于防篡改。

## 核对与离线重放

运行实例通过 `/v1/system/outbox` 提供 origin、待确认条数/字节、最大序号、投递验证时间及最后错误。`outbox_committed_total` 是提交数，`redis_xadd_total` 是逻辑确认行数，重放已有行也可以得到确认；它不是 Redis 物理新增条数。`redis_pending_events` 为耐久路径当前 pending，`redis_delivery_failures_total` 记录失败尝试。旧 `redis_accepted_total` 的本地队列口径只适用于 legacy 模式。

停止实例后，在相同配置/stream prefix 下执行：

```bash
market-bridge --outbox status data/events.outbox.sqlite
market-bridge --outbox export data/events.outbox.sqlite pending.jsonl
market-bridge --outbox replay data/events.outbox.sqlite pending.jsonl
```

导出文件独占新建，包含 origin、最大已分配序号、逐条记录 CRC、头部和内容的累计 CRC、条数及结束标记，刷新并同步后才返回成功。重放在一个事务里核对 origin、namespace、顺序、现有同 ID 内容、字节预算和完整 footer；不完整、冲突或超额会全部回滚。同一文件重复重放不会重复加入本地 pending。它仅把记录放回 outbox；正常启动后 worker 才投递 Redis。

导出只包括待确认事件，不是全部历史的替代品。当前离线重放要求保留原数据库身份；如果数据库完全丢失，须从一致性备份恢复 origin/最大序号，再使用导出文件补齐 pending。不能创建一个随机新 origin 的空库后声称还原了原来的事件身份，也不能单独复制导出 payload 后 `XADD *`。

## 已执行验证

SQLite 测试覆盖重启身份、已 ACK 序号不复用、第二实例租约冲突、pending 预算失败整批回滚、真实 SQLite page 上限失败、CRC 损坏、跨 origin 拒绝、截断导出回滚、重复重放，以及提交失败不发布/提交成功后总线与记录身份一致。

Linux 本地已构建固定 Redis 7.4.2 提交 `a0a6f23d997b024689ba157916837f493a593a34`。真实 Redis 测试覆盖：AOF 同步成功、远端已写但本地未 ACK、强杀/重启恢复、两次 outbox 重放无物理重复、去重证据被删后拒绝、未启用 AOF 时拒绝把普通 ACK 当耐久确认。Linux CI 配置了同一测试；Windows/macOS 运行共享 SQLite/事件契约测试，未声称本机运行了这些系统或 Redis 集群。

```bash
MARKETBRIDGE_TEST_REDIS_SERVER=/path/to/redis-server \
  cargo test --locked --all-features durable_outbox_against_real_redis -- --ignored
```

本阶段本机结果：427 项普通 Rust 测试通过，1 项外部 Redis 测试在普通套件中显式忽略、随后使用上述真实 Redis 单独执行通过；Clippy、locked 构建、6 项进程/离线 CLI 测试、13 项 SDK 和 529 项示例测试通过。HTTP 的 9 个研究模型和 SQLite 重启恢复验证通过。这些是 Linux 本地证据，尚未读取远端多平台 CI 结果。
