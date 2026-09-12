# 05 · 录制、完整文件回放与历史数据集

系统区分三种历史：K 线分区、规范化事件日志、带完整研究假设的场景数据集。
三者不能互相冒充。

## 规范化事件录制

启动前设置 `$env:MARKETBRIDGE_RECORD_DIR='data/recordings'`。程序保存本次接收到的
规范化 DataEvent，不是交易所原始 WebSocket 字节。队列有界，记录带序号和 CRC；
单行小于 1 MiB、单会话约 256 MiB 上限。到达容量上限会停止采集，不悄悄删历史。

正常 Ctrl+C 退出后日志封存为 `.jsonl`。强杀或故障可能留下 `.partial`。
不要仅修改扩展名来伪装封存成功。

```powershell
.\target\debug\market-bridge.exe --verify-journal data/recordings/session-EXACT-ID.jsonl
.\target\debug\market-bridge.exe --replay-journal data/recordings/session-EXACT-ID.jsonl route.json
```

`route.json` 是单条 LiveScanRequest，不是含 candidates 的整个批量请求。
可从 `examples/research/scan-live.json` 提取 candidates[0].route，并填写自己验证过的条件。

完整文件回放逐条读取匹配的现货订单簿，以当时接收时间推进决策，并计算同资产成本曲线。
不需要把整个文件加载到内存。只保留计数、时间范围、最大条件边际和最后结果。
损坏、未封存、非零 drop counter、观察时钟倒退、没有匹配双边行情等均拒绝。

“严格回放通过”只说明记录满足这些规则，不证明上游从未丢数据或价格可成交。
positive_conditional_observations 是重叠观察次数，不是交易次数或可累加收益。
该 CLI 不自动写实验库；保留报告和原始日志一起管理。

## 场景数据集

workspace `dataset_append` 保存完整 ScanRequest 列表，包含身份、关系、成本、证据与时间。
每块最多 512 帧、每集最多 100 块，且受单文档 2 MiB 限制；按时间顺序追加，不可覆盖。
`dataset_replay` 用 after_sequence + limit_chunks 分页回放并归档结果，页间不继承持仓。
详情及请求见 [工作区](01-workspace.md)。

需要持仓连续模拟时，明确构造 `allocated-spot-portfolio/v1` steps，而不是把无持仓的
成本曲线结果相加。超过单次上限的长期组合模拟仍需要外部研究驱动程序；当前没有
无限规模、跨页自动状态传递的回测引擎。

## 历史实验纪律

先按时间划分训练/验证窗口，再选择阈值；记录失败、没有机会和数据不足的样本。
不要仅保存赚钱区间。记录连接器/代码版本、数据来源、成本版本、资产目录修订及所有
填充假设。当前实验保存软件包版本和编译时 build_revision。CI 编译注入 Git SHA；
未注入的本地构建显示 unattested-build，不能伪称对应某次干净提交。详见运维手册。
