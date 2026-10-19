# MarketBridge 实战可靠性与跨平台性能复核

日期：2026-10-19。目标是普通 Linux/VPS，同时保留 Windows、macOS 的共享实现。

当前架构适合继续发展为多市场研究数据服务，但已有可复现的运行问题，也缺少完整链路的容量证据，不能称为“已经最优”。本轮优先处理退出语义、任务监督、缓存热路径和测量可信度；不引入实时内核、专用硬件或新的应用依赖。

## 已落实的改动

| 问题与证据 | 改动 | 验证 |
|---|---|---|
| API 端口已占用时，旧进程打印绑定失败但返回 0。 | 在启动采集、打开录制之前绑定 API；启动失败返回非零状态。 | 真实子进程占用端口测试，确认没有创建录制目录。 |
| Unix 的普通 SIGTERM 直接终止旧进程，录制只留下 `.partial`。 | Linux/macOS 的 SIGTERM、SIGINT 统一进入取消、排空和封存流程；Windows 保留 Ctrl-C 路径。 | 子进程分别接收两种 Unix 信号，校验退出码和封存文件 CRC/序号。强制杀进程仍不能保证封存。 |
| `TaskGroup.required` 原先只在停机时按注册顺序等待，无法及时发现后注册任务已经退出。 | 并发观察任务句柄；已启用的后台服务意外结束会触发失败退出；停机时等待其他服务并保留 panic 错误。 | 首个服务仍运行时检测后续服务退出；panic 后仍等待其他服务；无效 Redis URL 的真实进程失败测试。 |
| `SnapshotMap::prune_by_ts` 在每条事件上扫描整个 domain 缓存，标的越多，每条更新越贵。 | 每个缓存共享 1 秒清理周期，时钟回拨时重新检查；每次更新仍检查容量上限；查询仍按当前时间计算新鲜度。 | 清理调用次数、跨 clone 共享周期、容量上限、时钟回拨、静默源过期测试。 |
| 通用事件及部分 domain 广播发生在快照更新之前，消费者可能先收到通知再读到旧缓存。 | 先更新对应快照，再广播通知。 | 保留各 domain 的快照/订阅测试，增加单分片丢失、顺序和关闭后排空测试。 |
| 旧压测只生成两种 symbol 的报价，忽略参数错误、排空超时，丢失后仍成功退出。 | 四类负载、可控标的数/盘口深度、完整送达核对、显式失败状态、可复用矩阵脚本。 | 四类负载执行；单线程确定性丢失测试；CLI 非法参数和退出状态测试。 |
| README 建议固定 Rust，但开发与 CI/发布使用方式不一致。 | `rust-toolchain.toml`、CI、五个发布目标统一 1.97.1；CI 加入 macOS。 | 本机取消工具链环境覆盖后，确认仓库配置仍选中 1.97.1；Linux 全套检查通过。其他 OS 仍需要各自 CI 结果。 |

实现位置：[main.rs](../src/main.rs)、[event_snapshots.rs](../src/event_snapshots.rs)、[event_bus.rs](../src/event_bus.rs)、[load_test.rs](../src/load_test.rs)。

清理频率降低后，已过保留期限的内存条目可能等待下一次清理周期才被回收；它们不会因此重新变成新鲜数据。容量检查没有改为定时检查。超出容量时的排序淘汰仍然有成本，不能据此声称每次更新都有严格的常数时间上界。

启用的可选组件也受到监督：例如配置了 Redis，却因无效 URL 立即退出，不再默默保留一个缺少该组件的“正常”进程。连接重试中的任务仍被视为运行中；任务监督不等同于已经具备数据源健康/存储就绪探针。

## 测量方法与结果

本机为 Linux 6.18.55 x86_64 云容器，cgroup CPU 配额为 4 核、内存上限 32 GiB。使用 Rust 1.97.1 的 `cargo build --release --locked`，正式矩阵固定 3 个 Tokio worker，加上独立的压测生产主线程，共享 4 核配额。内核配置没有做 A/B 切换，也没有测试 PREEMPT_RT。

矩阵每个场景发布 100,000 条事件、8 个消费者、每分片 131,072 个广播槽位。每种构建每场景独立预热一次，再执行三次。每次都从空缓存启动，预热属于独立进程，不代表复用了上一轮的应用缓存。四分片场景的总缓冲预算大于单分片，不能当成相同内存预算的分片比较。

这里测量的是 **EventBus 更新快照及进程内订阅送达**。不包含交易所网络、解码、SourceRuntime、EventRouter、真实聚合服务、Redis/SQLite 写入，也没有触发按需 JSON 序列化。生产速率包含合成事件生成；发布调用延迟只计时 `publish_shared_event`。它不是 wire-to-API、wire-to-strategy 延迟，也不是硬实时截止时间。

以下为三次运行的中位数；p99 列是每次运行 p99 的中位数，不是合并样本后的 p99。该 100,000 事件矩阵中的所有发布调用都计时，完整原始结果保留预热、最大样本、排空时间、送达和丢失状态。

| 负载 | 基线发布 events/s | 最终发布 events/s | 发布速率比 | 消费送达 messages/s（前 → 后） | 发布调用 p99 μs（前 → 后） |
|---|---:|---:|---:|---:|---:|
| 报价 / 2 标的 / 1 分片 | 237,485 | 280,253 | 1.18× | 1,885,259 → 2,221,097 | 20.21 → 17.16 |
| 报价 / 1024 标的 / 1 分片 | 135,629 | 249,843 | 1.84× | 1,079,344 → 1,974,488 | 25.10 → 19.05 |
| 成交 / 1024 标的 / 1 分片 | 308,712 | 383,647 | 1.24× | 2,440,696 → 3,007,775 | 12.15 → 16.21 |
| 盘口 / 1024 标的 / 1 分片 | 266,559 | 231,434 | 0.87× | 2,110,777 → 1,836,305 | 12.97 → 26.63 |
| 混合 / 1024 标的 / 1 分片 | 174,293 | 572,412 | 3.28× | 1,382,298 → 4,451,183 | 22.60 → 12.80 |
| 报价 / 1024 标的 / 4 分片 | 138,335 | 323,354 | 2.34× | 1,087,945 → 2,492,023 | 25.03 → 15.24 |

完整顺序矩阵中的盘口发布速率回退约 13%，成交和盘口的发布 p99 也出现回退；没有把这些数据删掉。为了避免仅凭三个样本和运行时段判断，随后对关键场景进行了下述交错复核。

四种构建合计 72 次正式测量和 24 次预热。每次均核对 800,000/800,000 送达，零丢失、零未确认、无排空超时；正式测量未使用 `--allow-loss`。

原始数据：[基线](performance/2026-10-19-baseline.jsonl)、[仅清理优化](performance/2026-10-19-retention.jsonl)、[直接订阅候选](performance/2026-10-19-direct-candidate.jsonl)、[最终实现](performance/2026-10-19-selected.jsonl)。文件记录二进制 SHA-256、运行时 checkout 状态和参数；checkout 版本描述调用脚本时的目录，不能单凭它证明被测试二进制的来源。

构建对应关系：基线 `c8dd8e8`；清理优化 `fa91772`；直接订阅候选 `69b37cc`；最终实现 `27206a6`。基线已经包含生命周期修复和新压测工具，EventBus/快照热路径仍与初始实现一致。


推送前 GitHub 的邮箱隐私保护拒绝了九个尚未发布的代理提交，因此仅替换了这些提交的作者/提交者身份，保留每个提交的 tree、消息、时间和原有仓库历史。原始压测文件继续保留当时记录的提交编号；[提交编号映射](performance/2026-10-19-revision-map.json)将旧编号对应到可检出的新编号。原始测量及二进制 SHA-256 没有改写。

### 重点场景的交错复核

对宽标的报价和盘口，基线、仅清理优化、最终实现交错执行七轮，每轮轮换或反转顺序。每次 200,000 事件、8 消费者、262,144 槽位、3 workers 加生产主线程；共 42 次运行，每次核对 1,600,000 条送达，全部零丢失。延迟按固定间隔 2 采样，共 100,000 个发布调用样本。

| 1024 标的负载 | 基线 events/s 中位数 | 最终 events/s 中位数 | 同轮速率比的中位数 | 速率改善轮数 | 发布调用 p99 μs 中位数（前 → 后） |
|---|---:|---:|---:|---:|---:|
| 报价 | 136,255 | 353,979 | 2.09× | 7/7 | 23.79 → 14.65 |
| 盘口 | 235,740 | 444,898 | 1.67× | 7/7 | 18.42 → 16.32 |

“同轮速率比的中位数”先对每轮的最终/基线速率求比，再取中位数，因此不等于两列中位速率直接相除。报价与盘口各七轮均改善，但速率区间仍明显波动：盘口基线约 184–318k/s，最终约 245–539k/s。初始矩阵的负向结果和交错复核的正向结果共同说明，这些是具体负载与机器上的观测，不是每次运行都会达到的保证。

[交错复核原始数据](performance/2026-10-19-budget3-interleaved.jsonl)保留所有轮次；该组数据是本轮对报价/盘口改进更有代表性的证据。整个复核仍然只覆盖总线与快照，尚未证明完整采集/聚合/存储链路的容量。

### 没有采纳的优化

尝试过单分片直接订阅，移除转发任务与额外队列。首轮结果出现负载相关的反向变化，因此又对两标的/宽标的报价、混合负载及 1/8 消费者交错测试，每种组合重复七次；该诊断实验采用 4 workers 加生产主线程，与下表正式矩阵的 CPU 预算不同。

在该交错实验中，两标的、单消费者的直接订阅候选中位生产速率约 272k/s，而仅清理优化版本约 389k/s；其他场景存在改善或接近持平。该候选还同时改变了通知顺序，这一比较不足以把全部差异归因于队列本身。结果支持“没有稳定的通用收益”，并不支持“直接订阅总是慢”。

最终保留原有缓冲订阅默认和先更新快照再通知的顺序。移除额外队列还会改变慢消费者何时观察到 `Lagged`，在没有完整数据缺口标记之前，不宜把这种行为变化包装成默认性能升级。

[交错实验原始数据](performance/2026-10-19-interleaved.jsonl)保留了正向与负向结果。最终表格以最终构建重新执行的矩阵为准。云机器上的三次运行不足以建立工业容量上限或对 p99.9 作可靠承诺。

### 复现实验

各提交均使用 Rust 1.97.1（旧提交可用 `cargo +1.97.1 build --release --locked`）。先从对应提交构建优化二进制，将各版本二进制保存在 checkout 外，避免下一次构建覆盖。不要覆盖已有工作文件或重置有未提交改动的目录。

```bash
cargo build --release --locked
python3 scripts/benchmark_event_bus.py --label my-build --events 100000 --workers 3 \
  --output examples/out/my-build-benchmark.jsonl
```

脚本在 Linux/macOS/Windows 使用相同的 Python 标准库和子进程调用方式；已有结果文件不会被覆盖。固定 worker 数只影响被测子进程，不会改机器调度策略。

单独验证一个 workload：

```bash
./target/release/market-bridge load-test --events 100000 --subscribers 8 \
  --symbols 1024 --profile mixed --book-levels 20 --broadcast-capacity 131072
```

Windows 对应可执行文件为 `target\release\market-bridge.exe`。默认出现订阅丢失、未核对的送达或超时即非零退出；`--allow-loss` 仅用于显式过载实验，仍报告丢失，也不允许不完整核对通过。

## 后续阶段已落实的修复

本文上面的 EventBus 对照是较早构建的局部证据，不适用于加入事件身份与耐久投递后的完整服务。后续已经完成全局保留预算、增量成交清理、淘汰质量标记、稳定身份与 durable outbox、原生序号/真实就绪、有限成交窗口恢复，并提供实际 server/SQLite/Redis/HTTP 的持续验收工具。

当前行为以[机构使用门槛](institutional_readiness.zh-CN.md)、[源契约](source_readiness.zh-CN.md)、[窗口恢复](verified_trade_windows.zh-CN.md)和[完整链路验收](pipeline_acceptance.zh-CN.md)为准。仍需逐适配器原始协议核验、目标操作系统/卷/发行包验收和实际机构 RPO/RTO/SLA 测试。不能沿用本文的总线 events/s 当作当前耐久链路吞吐。

## 普通 Linux/VPS 与 PREEMPT_RT 的取舍

Linux 官方文档描述的 PREEMPT_RT 主要通过可抢占锁、优先级继承及线程化中断，降低高优先级任务从可运行到获得 CPU 的延迟。它没有消除应用内全表扫描、锁竞争、磁盘等待、TCP 重传、交易所合并推送或云宿主机争用。

Tokio 1.52.3 的调度保证以任务数量和每次 poll 的执行时间有界为前提；它没有提供固定的微秒级截止时间。当前系统还包含动态分配、JSON、共享缓存、异步队列和同步存储，所以安装实时内核不能把整个系统变成硬实时系统。

当前 [Binance 盘口连接器](../src/connectors/cex/binance.rs) 包含 `depth20@100ms` 订阅。源本身的推送节奏和公网路径需要单独计入新鲜度预算；本机发布调用的几微秒变化不能替代这些成本。

普通 VPS 建议顺序：

1. 使用 release 构建，明确订阅范围和各数据源频率，先保证源新鲜度、任务存活和数据缺口可见。
2. 同时观察发布、完整消费、排队、丢失与 RSS。以 100k events/s 为例，65,536 槽位在消费者完全停顿时约 0.655 秒就会填满；大队列也会积累过时数据。
3. 以 CPU 配额为依据比较 Tokio worker 数，保留磁盘、网络和 API 的处理空间。正式矩阵使用 3 workers 加生产主线程，另有 4 workers 的候选诊断；没有证明应用默认线程数应如何调整。
4. 只有当真实工作负载证明主导尾延迟来自 OS 调度，且有可控的机器/内核和明确目标时，才安排标准内核与 PREEMPT_RT 的对照实验。

本轮没有修改实时调度优先级、绑核、网卡参数或内核。`SCHED_FIFO`、忙轮询、隔离 CPU、DPDK/io_uring 等不作为默认安装要求。需要它们时应独立验证，并把平台实现收在可选适配层，保持数据模型、协议和研究逻辑共享。

## 多平台验证与日常使用

| 平台 | 实施情况 | 本轮证据边界 |
|---|---|---|
| Linux/VPS | 共享 Rust 实现；SIGTERM/SIGINT；systemd 示例；锁定工具链。 | 本机实际执行测试、构建、HTTP/SQLite 重启验证和合成压测。 |
| macOS | 共享 Rust 实现；Unix 信号；增加 CI；保留 Intel/Apple Silicon 发布目标。 | 已配置 CI，尚未在本机运行 macOS。 |
| Windows | 共享 Rust 实现；Ctrl-C；可移植 Python 进程/CLI 测试；保留 MSVC 发布目标。 | 已配置 CI，Unix 信号测试明确跳过；没有声称原生 Windows Service Control Manager 支持。 |
| Linux i686 | 保留现有交叉发布目标及 multilib 配置。 | 本轮未执行 32 位运行测试。 |

最小本地研究流程不需要 Redis、Node.js、行情密钥或实时内核：

```bash
cargo build --release --locked
MARKETBRIDGE_CONFIG=config.research.yaml ./target/release/market-bridge
```

PowerShell：

```powershell
cargo build --release --locked
$env:MARKETBRIDGE_CONFIG = "config.research.yaml"
.\target\release\market-bridge.exe
```

从仓库根目录执行，工作台随二进制嵌入。开启实际行情时再使用经过审阅的本地配置和对应网络权限。以上本地配置不提供实时市场数据，本轮没有把公网行情可达性、真实交易所负载或实盘策略效果列为已验证。

验证命令：

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
python3 -B scripts/Test-ProcessLifecycle.py
python3 -B -m unittest discover -s sdk/python -p 'test_*.py'
python3 -B scripts/run_python_example_tests.py
```

上一轮本机结果：400 项 Rust 测试、13 项 Python SDK 测试、529 项 Python 示例测试、4 项进程测试均通过，Rust 格式检查及 Clippy 通过。另外完成 9 个归档研究模型、同步/异步真实 HTTP 调用、CLI/API 一致性、工作台/CSP、SQLite 完整性和进程重启恢复验证。

## 参考资料

- [Linux 官方 PREEMPT_RT 原理](https://docs.kernel.org/core-api/real-time/theory.html)，核对其 [内核仓库源文档](https://github.com/torvalds/linux/blob/master/Documentation/core-api/real-time/theory.rst)。
- [实时内核的差异](https://docs.kernel.org/core-api/real-time/differences.html)，核对其 [源文档](https://github.com/torvalds/linux/blob/master/Documentation/core-api/real-time/differences.rst)。
- [项目锁定的 Tokio 1.52.3 runtime 调度说明](https://github.com/tokio-rs/tokio/blob/tokio-1.52.3/tokio/src/runtime/mod.rs)，重点为 scheduling fairness、poll 时间前提和 I/O/timer 调度。
- [Rust 工具链配置](../rust-toolchain.toml)、[CI](../.github/workflows/ci.yml)、[发布矩阵](../.github/workflows/release.yml)。

外部参考核对日期：2026-10-19；内核文档的上游 master 会继续变化。
