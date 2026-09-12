# 04 · 自动扫描、告警和配置热重载

扫描器只读已有缓存，不下单，不自动建立行情订阅，也不拉取每条路线的外部 REST 数据。
启动采集器后再配置路线。默认关闭，避免空配置悄悄运行。

## 配置入口

GET `/v1/research/control` 读取 `config`、`latest`、`reload_error`。
POST 同一路径应用完整 JSON 配置。请求带 API Key，配置校验和持久化成功后才替换。

```json
{
  "revision": "scanner-001",
  "enabled": false,
  "interval_ms": 5000,
  "min_net_bps": 2,
  "min_hold_ms": 10000,
  "record_scans": false,
  "record_announcements": false,
  "routes": []
}
```

开启时 `routes` 必须有 1–64 条，形如 `{"id":"route-a","route":{...}}`。
route 的字段为 `buy`、`sell`（完整 Instrument），`relationship`，`quantities`，`costs`，
`max_age_ms`，`max_skew_ms`。现有 `examples/research/scan-live.json` 的 candidates 中
包含同样的 route 结构，可复制后补齐自己验证过的身份和成本。

| 字段 | 约束/行为 |
|---|---|
| revision | 1–120 位安全 ID；变更内容必须新版本；相同最新版本相同内容幂等 |
| interval_ms | 1000–60000；是目标周期，不是硬实时保证；不会并行堆积扫描 |
| min_net_bps | 非负；同时要求条件净额大于 0 |
| min_hold_ms | 0–3600000；以单调时钟累计连续合格时间 |
| record_scans | 每轮持久化到 captures，包括输入证据；注意数据库空间 |
| record_announcements | 从已启用外部信号流中提取带 title/url 的公告，保存首次观察 |

数据缺失、变旧、条件不满足会中断连续合格时间。切换版本重置持有计时。
每条路线有自己的决策时刻，不能声称跨平台原子快照。

## 告警消费

合格路线集合变化时写入 `events`，包含 `scanner_state_change`、配置版本、证据和
`not_an_execution_signal=true`。相同状态不重复发送。通过 workspace `list` 及 SDK
游标订阅消费；本轮没有内置邮件、短信、Telegram 发信或交易执行。

配置修订会重置扫描状态，告警不是“永不重复”的全局消息队列。
启动、停止或修订配置时会记录 `scanner_reset`，清空消费者此前保存的合格路线集合；
消费者不能把停止前最后一条正向观察永久保留为有效状态。
消费者保存处理完成后的 sequence，以幂等方式恢复。

## 文件热重载

启动前设置 `MARKETBRIDGE_CONTROL_FILE` 指向同结构 JSON 文件。程序约每秒读取一次，
上限 2 MiB。编辑器最好以临时文件替换完成写入；写到一半的无效 JSON 会被拒绝，
旧配置继续工作，错误留在 `reload_error`。修复后使用新的 revision。

不要同时用文件和 API 维护配置，避免来源互相覆盖。回滚旧内容也使用新 revision。
文件热重载仅覆盖研究扫描配置，**不覆盖**主 YAML 的采集器、API 监听、鉴权、数据库、
源凭证或队列参数；那些变更需要正常停止再启动。

扫描记录写入失败会暂停本进程扫描，状态 `paused_storage_error`；停止、备份并修复空间后
再操作。持久化的 enabled 配置在重启后会恢复，因此排障重启前应确认可用磁盘。
