# 源连续性与真实就绪

`/health` 只证明 HTTP 进程存活。`/ready` 与 `/v1/system/readiness` 检查已配置 source 的实际数据接收、适配器报告的源时间、router/bus/策略与成交消费者进度，以及耐久投递验证；未就绪返回 503。空采集配置是 `research_only=true`，可就绪但 `data_ready=false`。模型预热仍需单独检查。

`runtime.readiness.max_age_ms` 默认 60 秒；`source_max_age_ms` 可按 source label 或 `label#instance` 覆盖；`required_streams` 用 `exchange:domain:market:symbol（资金费率/OI 等无 market 段）` 指定必须新鲜的流。没有声明的标的覆盖不作保证。消费者进度比较实际进入该消费者的工作；没有成交而持续收到报价，不会误报成交消费者卡死。心跳、任务重试和收到行情分别统计。

默认 `require_complete_analytics=true`：源丢失、订阅缺口或策略容量损失会阻止数据就绪。关闭该选项只放宽运维 readiness，不能清除质量证据或解除策略门槛。历史原始成交和桶淘汰单独报告，不把有界历史误称完整历史。

## 原生序号策略

仅 Binance 标准化 aggTrade 和明确标注的 `acceptance_fixture` 成交采用连续原生 ID 检查。完全相同的相邻 ID/价格/数量/时间/方向被去重；冲突、倒退被隔离；跳号记录损失。深度快照的 update ID 可以跳跃，不以相邻 +1 误判。其他交易所仍报告上游连续性 `not_verified`，不能因为本地没丢就声称完整。

先取得有界队列许可，再推进原生检查点。排队取消、丢弃、非法值、时间倒退等进入不可自动消除的 `SourceIntegrity`；原因计数可能重叠，不能相加当成唯一丢失事件数。`observation_origin` 与 `loss_epoch` 限定到当前进程。开启 outbox 时，原生检查点与事件同事务提交，ACK 删除事件后检查点仍保留，重启加载并验证 CRC。源在进入此进程之前丢失的数据仍不受保证。

事件携带接收时的质量快照，进入总线、Redis/outbox 和 journal。派生状态读取共享质量，`squeeze-radar/v4` 与旧 leg 在已知损失时禁止触发。journal/v2 增加完整性计数，仍可读取 v1；严格全文件策略重放拒绝任何已记录丢失/完整性问题。CRC 检测损坏，不是签名。

## 策略历史语义

数值历史每分钟保留最新观察，`numeric_history_sampling_ms=60000`；资金费率历史计数是分钟观察数，不是结算次数。基线必须满足完整请求时长，允许的最近基线偏差最多一分钟或窗口的 1%；45 分钟数据不再冒充一小时预热。成交流仍保留毫秒时间，受全局容量限制。

价格参考优先级为永续盘口中价、永续 mark、永续成交、现货后备。切换到更高优先级会清空价格基线并重新预热，响应展示 `price_reference`。强平价格不刷新当前市场价；不同产品的价格不混为同一个收益序列。

这些约定经过本地进程/实际 Redis 测试；它们不构成交易所覆盖、其他操作系统实机验收或生产 SLA 证明。

订单簿另有独立连接 lease 与 OFI 重建门槛，见[订单簿契约](reconstructed_order_books.zh-CN.md)。
