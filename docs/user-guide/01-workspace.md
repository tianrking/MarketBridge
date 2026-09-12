# 研究工作区、资产目录与实验归档

## 存储与启动

默认数据库为 `data/research-workspace.sqlite`。可通过环境变量
`MARKETBRIDGE_RESEARCH_DB` 指向独立数据库。使用 SQLite 事务与 FULL 同步；
记录不可覆盖，查询时检查 CRC。CRC 检查损坏，不证明信息真实。

每个文档上限 2 MiB，数据库页容量上限约 512 MiB。容量不足会报错，不自动
删除实验。停止服务后备份数据库；不要在写入过程中只复制单个数据库文件。

统一入口：`POST /v1/research/workspace`。请求格式：

```json
{"action":"integrity"}
```

常用操作：

| action | request 字段 | 作用 |
|---|---|---|
| `registry_put` | `id, known_at_ms, evidence, instruments, relationships` | 保存不可变资产目录版本 |
| `registry_pair` | `revision_id, left, right, as_of_ms` | 按知识时间和有效期查询有向资产关系 |
| `dataset_append` | `dataset_id, chunk_id, frames` | 追加按时间排列的 ScanRequest 分块 |
| `dataset_replay` | `dataset_id, run_id, after_sequence, limit_chunks` | 回放已保存分块，归档结果 |
| `run` | `id, model, input` | 执行模型，保存完整输入、结果或错误 |
| `list` | `namespace, after_sequence, limit` | 按序号分页读取 |
| `get` | `namespace, id` | 读取单个记录 |
| `integrity` | 无 | SQLite 结构检查；文档 CRC 在读取时检查 |

ID 只能使用英文字母、数字、`-_.`，最长 120 字符；数据集 ID 最长 80 字符。
重复 ID 会报错。所有错误都必须由客户端显式处理，不自动换 ID 重试写入。

## 资产目录

资产不是一个 ticker。Instrument 包含发行方、链/地址、单位、产品类型、
合约乘数和结算语义。目录内禁止重复资产 ID、未知引用、自关联，以及与资产
语义冲突的 `same_asset` 关系。不同发行方、转换关系和对冲关系必须明确区分。

`known_at_ms` 是调用者对知识可用时间的声明，`recorded_at_ms` 是本服务真实记录
时间，两者都会保留。导入旧资料不等于独立证明它在历史时点已经可知。研究者
必须核实来源；不能把事后整理的关系冒充当时已经掌握的信息。

## 从样例保存实验

在项目根目录的 PowerShell 中：

```powershell
$evidence = Get-Content examples/research/same-asset.json -Raw | ConvertFrom-Json
$request = @{action='run'; request=@{id='first-experiment'; model='same-asset-spot/v1'; input=$evidence}}
Invoke-RestMethod http://127.0.0.1:8080/v1/research/workspace -Method Post -ContentType application/json -Body ($request | ConvertTo-Json -Depth 50)
```

读取实验：

```json
{"action":"get","request":{"namespace":"runs","id":"first-experiment"}}
```

模型校验失败也会存为 `output.status=failed`，不会从研究结果里消失。
存储自身失败则不会返回保存成功。文档中包含模型、输入、输出、开始/结束时间
和包版本。提交 SHA 仍应由实验者随部署记录保存，不能仅用包版本区分源码。

## 数据集分块与回放

`frames` 的元素与 `/v1/research/evaluate` 输入相同，每块 1–512 帧、最多
100 块，块内及块间须按 `as_of_ms` 非递减排列。未来接收的数据会被拒绝。
这种数据集保存完整研究输入，不是原始交易所 WebSocket 帧归档。

`dataset_replay` 每次最多请求 16 块；结果文档同样受 2 MiB 限制，超限时应减少
`limit_chunks`。使用返回的 `next_sequence` 继续回放，不是从头重复。不同窗口
的结果不得伪称为一次带有跨窗口持仓状态的组合回测。

`list` 每页最多 100 条且约 4 MiB，使用 `next_sequence` 继续。命名空间包括
`registry`、`runs`、`events`、`control`、`dataset.<数据集ID>`。

本模块不自动下载全市场历史、不证明第三方资产等价、不进行真实账户核对。
