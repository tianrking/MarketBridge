# 当前云环境：完整链路验收记录

2026-10-19；普通 Linux 6.18.55，cgroup CPU 配额 4 核、内存上限 32 GiB，Rust 1.97.1 release，Redis 7.4.2（固定上游提交 `a0a6f23d997b024689ba157916837f493a593a34`）。16 个合成标的，6 成交 / 2 十档盘口 / 1 funding / 1 OI 的十轮混合负载。SQLite FULL/WAL；Redis everysec + WAITAOF。没有外部交易所网络、下单或 RT 内核。

测量对应代码 `8d69505`，完整源码 SHA256、二进制 SHA256 与参数见各目录 `metadata.json`。二进制标签为提交前的 `304f2b7-plus-acceptance`，不能将该标签当成最终提交号；以 SHA256 与源码清单复核。三轮使用同一二进制，单独顺序运行，无并发编译/压测。常驻的旧研究服务保持空采集；测试完成后换成当前实现，原研究归档核对不变。

| 负载 | 输入目标 | 输入及排空实测速率 | 核对事件（含重启后） | 服务峰值 RSS | Redis 峰值 RSS | 状态 HTTP 采样 p95 | 策略阶段 p99 桶上界 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 持续 600 秒 | 1,000/s | 999.82/s | 600,320 | 113.98 MiB | 416.87 MiB | 0.90 ms | 10 ms |
| 短测 30 秒 | 5,000/s | 4,980.87/s | 150,320 | 113.25 MiB | 108.07 MiB | 1.05 ms | 50 ms |
| 短测 30 秒 | 20,000/s | 19,273.66/s | 600,320 | 189.55 MiB | 407.96 MiB | 4.02 ms | 500 ms |

三轮逐条核对全局身份、CRC 和每标的成交原生 ID，均无缺失/重复/原生跳号；合计 1,350,960 条。每轮包含 Redis 中断 5 秒、AOF 重启、恢复排空、server 优雅重启、原生检查点继续及 API 窗口与 Redis 账本交叉核对。实际 Redis ACK 丢失/强杀/去重重放另由 Rust 集成测试覆盖，不把这些故障混作本次优雅重启。

600 秒运行的 source→聚合器均值 2.08 ms、source→策略 2.19 ms、source→order-flow 2.57 ms；后两者及聚合器 p99 桶上界均 10 ms。服务平均采样 CPU 约 0.108 核，Redis 约 0.033 核；不包括宿主机和页缓存。服务最后 120 秒 RSS 为 112.40–113.98 MiB，这只是该时间段，不能推为 24 小时稳态上界。

原始成交固定保留 250,000 条（约 43.78 MiB），累计淘汰 110,000 条，质量标记未清除。聚合器约 1.03 MiB、策略约 12.40 MiB，无容量淘汰。`sustained-1k/tainted-history-window.json` 额外验证：在已有原始历史淘汰时，5 秒近期窗口的 186 笔成交、买卖各 93 单位与 Redis 完全一致；`global_quality_cleared=false`、`lifetime_cvd_repaired=false`。

20k 输入出现明显排队和额外排空，峰值 outbox 积压 133,088 条，p99 桶上界 500 ms。它通过的是有界排空与正确性核对，不能宣传为 20k/s 低延迟持续容量。5k 仅测 30 秒，未覆盖最长历史窗口的稳态。没有进行旧 100 条批次与新批次的 A/B 压测；`WAITAOF` 并非每次等待一秒，不能推导“旧实现必然只支持 100/s”。本构建 Redis 操作计时包含空队列连接探测，不能把其所有样本当成纯 AOF 批次。

## 证据与复现

每个目录保留 `report.json`、摘要、配置、最终 readiness、直方图、有限窗口证明、压缩原始 `samples.jsonl.gz` 和进程日志。大体积 SQLite/AOF 数据未提交，仍保留于本次工作区的 `/workspace/.marketbridge/current-cloud-*`。`sha256.json` 可校验提交证据。日志与 samples 只做 gzip 无损压缩，没有选择性删除慢样本。

```bash
cargo build --release --locked
python3 scripts/accept_pipeline.py --binary target/release/market-bridge \
  --redis-server /path/to/redis-server --duration 600 --rate 1000 \
  --symbols 16 --sample-seconds 5 --output /new/result-directory
python3 scripts/summarize_pipeline.py /new/result-directory
```

首次长测在 Redis 重启的正常 `LOADING` 阶段被旧 harness 错判失败，原始失败见 `failed-redis-loading/`；`8d69505` 改为有时限等待加载完成，然后从零重跑了完整 600 秒。失败没有计入成功吞吐。

本轮功能回归：448 项普通 Rust 测试通过，1 项默认 ignored 的真实 Redis 集成测试单独执行通过；fmt、Clippy、locked debug/release 构建、7 项进程、13 项 Python SDK、529 项 Python 示例、9 个模型的真实 HTTP/CLI/SQLite 重启核对通过。Windows/macOS 共享实现与 CI 已配置，本机没有运行这两个系统。未执行 72 小时、外部交易所真数据或生产卷断电/灾备验收；机构上线仍需按真实流量、协议、存储和 SLA 验收。
