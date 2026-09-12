# 09 · 运行、备份、故障排查与发布验收

## 两个数据库，不要混淆

原有 K 线/数据湖路径由 YAML `klines` 配置决定；研究工作区由
`MARKETBRIDGE_RESEARCH_DB` 指定，默认 `data/research-workspace.sqlite`。
研究库使用 SQLite、完整同步提交、不可覆盖记录及每文档 CRC。
单文档 2 MiB，数据库页上限约 512 MiB；临时事务文件和其他日志另占空间。

workspace `integrity` 检查 SQLite 结构；每次读取校验 payload CRC。CRC 不是防篡改签名。
备份最简单的方法：Ctrl+C 停服务 → 复制确切数据库文件、配置、已封存日志 → 记录 SHA256
及 Git SHA → 用副本路径启动并查询归档。不要热复制一个正在写入的库就称为一致备份。
当前研究工作区按单进程使用，不承诺多进程追加同一个数据集的顺序事务。

不可覆盖设计没有后台自动删除。达到限额前停扫描，封存旧库，切换新库；清理须由操作者
明确选择备份后的具体文件。旧库的游标只在旧库内有效，新库不应继续沿用。

## 常见问题

| 症状 | 检查方法 |
|---|---|
| 401 | Key 是否一致，是否填了错误环境变量；不要靠关闭鉴权解决公网访问 |
| 422 | 查看 error：身份/时间/成本/大小/重复 ID 等校验拒绝 |
| 429 | 研究并发槽位或 API 限流；降低并发，不进行无限立即重试 |
| 数据为空 | 零采集器配置是正常行为；检查源开关、符号、网络和原始服务状态 |
| reference_only | 阅读 reasons；缺成本、参考价、过期/不同步数据不是盈利信号 |
| condition_not_met | 组合步骤门槛未达到，不是系统故障 |
| paused_storage_error | 检查磁盘/研究库上限；停止并备份，修复后再恢复 |
| reload_error | JSON 写入不完整、revision 重用或字段不合法；旧配置仍保留 |
| 启动后立即退出 | 端口冲突、数据库损坏/权限、必要后台任务错误；保留日志 |
| 日志只有 partial | 非正常封存；验证前缀，不能充当严格完整回放输入 |

## 本地验证命令

对干净候选提交，可在编译前设置 `$env:MARKETBRIDGE_BUILD_REVISION=(git rev-parse HEAD)`。
有未提交修改时不得把该 SHA 当成精确源码证明，应使用带 dirty 标记的值或保留默认
unattested-build。系统信息和新实验记录会保留这一构建标识。

```powershell
cargo +stable fmt --all -- --check
cargo +stable clippy --locked --all-targets --all-features -- -D warnings
cargo +stable test --locked --all-features
cargo +stable build --locked
python -B -m unittest discover -s sdk/python -p 'test_*.py'
pwsh -NoProfile -File scripts/Test-ResearchApi.ps1
pwsh -NoProfile -File scripts/Test-PublicResearchSources.ps1 -ObservationSeconds 20
pwsh -NoProfile -File scripts/Test-ResearchSoak.ps1 -DurationSeconds 60 -PublicSources
```

Python 测试先安装 SDK requirements。HTTP 脚本使用独立端口/Key/数据库，最后只停止自己
启动的进程。公共源测试会联网，但不会下单。测试生成文件在忽略的 examples/out 下。

Soak 脚本可指定 `-DurationSeconds 259200 -IntervalSeconds 5 -PublicSources` 做 72 小时观察。
它保存逐样本 JSONL、进程内存/CPU、HTTP 时延、盘口更新身份、错误、二进制哈希和 Git SHA，
日志上限 256 MiB，退出时停止自己启动的服务。不带 PublicSources 只测本地空源服务。
即使运行满时长，report 的 release_certified 仍是 false：故障注入、Linux CI 和数据许可等
其他验收须另行完成。退出前保持电脑运行，不要将睡眠期间算作持续行情观测。

## 发布不是改版本号

本轮新增代码是未发布开发功能；精确验收状态见[开发日志](../development-log.md)。
发布前必须检查[完整清单](../release-checklist.md)，至少包括：

- 对最终 Git SHA 的 Windows/Linux CI、测试和打包产物校验。
- 在目标机器上进行至少 72 小时观察，记录内存、CPU、磁盘、数据新鲜度、源错误和重连。
- 明确注入断线、源停顿、429、存储不足与进程退出，检查恢复结果。
- 解压真实发布包后运行；文档、配置、SDK 和校验和同步。
- 公共行情短时成功，不代表全部平台可用；数据许可与商用权限逐源审查。
- 当前仓库 README 有 MIT 徽章，但没有 LICENSE 正文；正式分发前需由仓库所有者确认许可并补齐。

未实际运行的长测、未检查的远端 CI、未发布的版本都不能标“通过”。
研究基座的验收也不等于策略盈利验收；论文级历史验证和真实交易是不同问题。
