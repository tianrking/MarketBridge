# 原生订单簿与自动恢复：当前云环境功能验收

2026-10-19，普通 Linux 云环境。生产 Rust 协议实现对应提交 `f34a3ef32c16c72716acf79e99231eed2e22171a`，每个 Rust 源文件 SHA256 已与该提交逐项核对。二进制 SHA256 为 `7a4e4b0ec0f8f0ecd7b93a0c37d97be9c1ff740adda021ecebe85f6f1adc3c5a`。metadata 同时记录独立验收脚本的 SHA256；构建发生在代码提交前，源码哈希用于准确关联实际构建。

这次验收启动真正的 MarketBridge 进程和回环 HTTP/WebSocket 协议对端；Redis 模式还启动真实 Redis 7.4.2 AOF。没有连接公网交易所，也不是新版本容量、延迟 SLA 或长期 VPS 运行证明。Binance spot 的完整进程链路在此验证；Binance USD-M、OKX、Bybit 的原生同步、checksum/序号和单位规则另由 Rust 状态机及实际回环协议测试覆盖，不声称完成三家公网实盘认证。

## 两种存储模式都通过

| 故障或检查 | 实际结果 |
|---|---|
| 每侧 1000 档 REST 快照，快照延迟使 WS 增量先缓冲 | 正确合并；删除一档后每侧保留并发布 999 档。fixture 显式设置 publish_levels=1000，生产默认仍为 200。 |
| 第 500 档附近的原生十进制价位更新、零数量删除 | 深层买档 94.00 数量为 7，卖档 106.00 数量为 8；98.99/101.01 被精确删除。 |
| 深度快照 ID=10、增量 11→13 | 旧簿 synchronized=false；新连接快照 20、增量 21 后恢复，session 必须改变。 |
| 成交 10→13，缺少 11/12 | 自动耐久建任务，冻结真实成交边界；范围扩展到闭合的一分钟窗口。 |
| 第一笔 REST 返回 429、Retry-After=1 | 自动重试；共 4 次请求，包含 fromId=0/1000 分页，获得 1503 行原生证据。 |
| 原生窗口前边界 0、后边界 1502 | 核验窗口内 1501 条，主动买量 751、卖量 750，delta=1，买卖名义额分别 7510/7500。 |
| 核验后进程重启、重复收到成交 13 | checkpoint 与证据保留；不重复下载已封存窗口，不创建重复任务。 |
| 在线 CLI status、离线 retry | status 可只读查询运行中的库；retry 被在线独占 lease 拒绝，停服后也不能修改已核验证据。 |
| 对原生记录 11 人为损坏 CRC 后再启动 | 窗口查询返回 503，recovered=null；不能只信曾经保存的 verified 标记。 |
| 恢复全局质量 | global_quality_cleared=false，lifetime_cvd_repaired=false；有限窗口补齐不修复终身累计 CVD。 |
| 真实 Redis AOF 成交流 | 仅实时原生 ID 10、13 各一条；补采历史与重启重复成交未混入。所有导出事件身份唯一、CRC 匹配。 |

## 证据与独立重算

`compact/` 是独立恢复库模式，`redis/` 是同库耐久 outbox + Redis AOF 模式。每个目录保留：

- `metadata.json`、`report.json`：构建、源码、脚本与检查结果。
- `initial-book.json.gz`、`invalidated-book.json.gz`、`rebuilt-book.json.gz`：完整千档结果、失效状态及新 session，包含精确档位字符串。
- `verified-native-records.jsonl.gz`：人为破坏之前导出的 1503 条原生证据，含 job ID、原生 ID、完整存储 body 与 CRC；body 内保留原始 REST JSON 对象及价格/数量文本。
- `recovered-window.json`、`restart-window.json`、`recovery-status.json`、`provider-requests.json`、`corrupt-proof-response.json`：窗口、冻结成交边界、重启、分页、限流及失败关闭证据。
- `redis/redis-events.jsonl.gz`：实际 Redis 导出事件及身份、原始 payload 字符串、CRC。

`manifest.sha256.json` 覆盖验收产物；日志使用 gzip 保存。重新计算文件哈希、原生行 CRC、序号/时间连续性、窗口 CRC 和原始十进制买卖量：

```sh
python -B scripts/verify_native_evidence.py docs/validation/2026-10-19-native-recovery-books
```

CRC 和这些本地证据不构成交易所签名或第三方认证。验收末尾故意损坏运行目录内数据库以验证拒绝行为；导出的 verified-native-records 在损坏前取得。

## 回归与范围

Rust 473 项普通测试通过，另 1 项默认忽略的真实 Redis AOF/ACK 丢失/重放测试显式运行通过；fmt、严格 Clippy、7 项进程测试、13 项 SDK 测试、529 项 Python 示例及 9 个研究模型的真实 HTTP/CLI/SQLite 重启回归通过。CI 添加 Linux 真实 Redis 原生协议验收，以及 Windows/macOS 独立恢复库协议验收；本次本地执行仅覆盖 Linux。验收工具在 Unix 使用 SIGTERM 优雅重启，在 Windows 使用 TerminateProcess 检查耐久崩溃恢复，停止方式写入 metadata。

`pipeline-regression/` 保留新增逻辑后的 30 秒通道回归：30320 个事件，包括 Redis 中断/重启与服务重启，稳定事件身份无重复/缺失。这个旧合成协议的盘口无原生重建 lease，不能据此声称千档 L2 或 OFI 的处理性能。既有 600 秒容量记录仍只对应原记录中的旧构建。

`research-service-restart.json` 记录本任务的本地研究服务更新：/health 和 /ready 均为 200，research_only=true、data_ready=false；原有 4 个归档文档逐字节哈希保持一致，另新增正常的 scanner_reset 启动事件。

恢复配置默认每任务最多 100000 行、范围最多 15 分钟，有限证据和任务会滚动淘汰；无法取得完整原生边界、历史缺失或超预算会明确拒绝认证。订单簿覆盖订阅及快照可知范围，不包括隐藏/RPI 流动性；重建当前簿不能补造断线期间的历史深度。部署配置和协议边界见 [自动恢复说明](../../automatic_trade_recovery.zh-CN.md) 与 [订单簿使用契约](../../reconstructed_order_books.zh-CN.md)。
