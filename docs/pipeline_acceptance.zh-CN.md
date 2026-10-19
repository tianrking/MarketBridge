# Linux/VPS 完整链路验收

使用实际 `market-bridge` 进程、合成 JSON 编解码 source、有界 source/router 队列、SpreadAggregator、两个派生 store、SQLite FULL/WAL outbox、Redis Streams/AOF 与 HTTP 查询。明确标注 `acceptance_fixture`，不伪装交易所数据。可选 ClickHouse、外部 kline 拉取、链上与期权采集未在这个负载模型中启用。

```bash
cargo build --release --locked
python3 scripts/accept_pipeline.py \
  --binary target/release/market-bridge \
  --redis-server /path/to/redis-server \
  --duration 600 --rate 1000 --symbols 16 \
  --output /path/to/new-acceptance-directory
```

需要 Redis >=7.2（`WAITAOF`）。工具只在本地回环地址启动自己的两个进程，清理时仅停止自己的子进程，输出目录必须不存在。Python 只使用标准库；Windows 需要可用的 Redis 服务端可执行文件，默认 binary 路径应显式改为 `.exe`。Rust 原生构建与常规测试在 CI 覆盖 Linux、macOS、Windows；本次资源与持续负载实测只覆盖 Linux。

负载按每十轮每标的 6 笔成交、2 个十档买卖盘口、1 次资金费率、1 次 OI 生成，成交含连续原生 ID。接收延迟从 fixture 编解码前开始；真实适配器的计时从调用 `emit` 开始，因此不包含该调用前的网络/解析时间。指标按固定 stage/domain 分类，不以标的作为 Prometheus 标签。

工具强制 Redis `appendfsync everysec`、本地 AOF 确认后 ACK；中断 Redis 5 秒，检查 `/health` 仍可用而 `/ready` 返回 503，再验证积压排空。随后优雅重启真实 server，追加少量事件，核对持久化 origin、全局序号与每标的成交序号。逐条读取 Redis 记录验证 CRC、缺失/重复及源序号，不仅比较最终计数；将 API 有限窗口的买卖量与耐久账本交叉核对。ACK 丢失与离线重放的幂等性另由实际 Redis 集成测试验证。

原始 `samples.jsonl` 包含就绪状态、预算、固定桶延迟直方图、HTTP 时延、磁盘与 Linux `/proc` CPU/RSS；`metadata.json` 记录源码和 binary SHA256、CPU quota、内存限制及参数。`report.json` 只有全部检查通过才产生 `passed=true`，失败保留 `failure.json` 和进程日志。直方图只能给桶上界，不能当作精确百分位。

当前合成 JSON 负载的盘口没有交易所重建 lease，新增 L2 门槛会将其标为未验证；它仍用于通道、成交、持久化和排空验收，不能用来证明原生 L2 重建或 OFI 性能。新增真实进程协议故障验收使用 `scripts/accept_native_protocols.py`，见[自动恢复说明](automatic_trade_recovery.zh-CN.md)。既有 600 秒容量记录对应其清单标注的旧构建，不能当作新增订单簿逻辑的性能结果。

## 持续运行与容量边界

`--duration` 可设为 259200 做 72 小时验收，但必须为实际速率配置足够的 Redis 内存和磁盘。默认 Redis 2 GiB `noeviction` 与输出目录 8 GiB 磁盘守卫会在资源不足时失败；不会悄悄裁剪账本以掩盖缺失。合成验收保留全部 Redis 证据，磁盘随总事件数增长，与 bounded outbox 不是同一层。可通过 `--redis-maxmemory-mb` 和 `--max-disk-mb` 显式预算；工具自身最终去重位图每事件约 1 字节，超大运行需计入验收机内存。请同时监控宿主机 Redis/AOF、日志、数据库备份与导出空间。

耐久读取批次最多 4096 条且通常不超过 4 MiB；单条大记录可以独占一批。旧模式仍采用原来的小批次。预算覆盖 store、聚合器保留的自有分配，不是进程 RSS 上限，查询临时对象、allocator 和网络缓冲需用实测 RSS 单独验收。聚合器容量淘汰会明确记录 `aggregator_capacity_evictions_total` 并使源质量不完整，禁止扫描器继续触发。

推荐部署使用进程监督、持久卷、明确 `required_streams` 与各流新鲜度；依照 `/ready` 调度数据消费者，依照策略模型自身的完整预热和质量字段决定是否可用。此工具是可重复验收证据，不是交易所真数据覆盖、所有适配器性能、硬实时或机构生产 SLA 证明。

实际三档负载、失败记录、资源与时延证据见[当前云环境验收记录](performance/2026-10-19-pipeline-current-cloud/README.md)。`passed=true` 表示正确性和有界排空检查通过，不等于达到输入目标速率或延迟 SLA。Redis 操作直方图在此测量构建中包含空队列 PING，详细样本含义以记录为准。
