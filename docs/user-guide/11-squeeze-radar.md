# 用 MarketBridge 寻找永续合约逼空结构

> 目标：建立一个**只读、可复核、可归档**的研究流程，寻找“空头拥挤后可能发生逼空”的永续合约结构。
>
> 非目标：本章不提供自动下单、钱包签名、借币、转账、真实账户管理或收益保证。`triggered_research_candidate` 只是“值得开始纸面研究”的状态，不是买入指令。

## 1. 先理解：什么是我们要找的结构

市场里常见的错误流程是：看到一个币资金费率为负，就直接判断“空头太多、要逼空了”。这是不成立的。负 funding 也可能只是价格持续下跌、流动性不足、合约与现货脱节，或某一交易所的临时定价问题。

MarketBridge 采用的是“从广到窄”的证据流程：

```text
全市场 Funding 海选
          ↓
同一交易所内识别异常负值
          ↓
确认现货 / 永续合约身份与流动性
          ↓
把少量候选接入实时观察池
          ↓
等待 OI、价格、CVD、盘口形成时间基线
          ↓
数据质量合格后，评估逼空共振
          ↓
归档证据 → 纸面实验 → 历史 / 持续验证
```

真正有研究价值的逼空假设通常是：

1. 永续合约持续负 funding，说明空头侧支付成本或合约存在明显做空倾向；
2. OI 在固定时间窗口内增加，说明杠杆仓位没有自然消退；
3. 现货主动买入而永续主动卖出，可能代表现货吸收卖压；
4. 永续订单簿出现买方承接，且已有空头回补/买方强平；
5. 价格已经走出右侧确认，而不是仍处于自由下跌。

这不是因果证明，只是一组可以被数据推翻或支持的条件。

## 2. 资金费率：先正确读数

`/v1/market/perpetual-funding` 返回字段 `funding_rate_pct`，已经是百分比：

| 返回值 | 含义 |
| --- | --- |
| `-0.05` | -0.05% |
| `-0.20` | -0.20% |
| `0.10` | +0.10% |

不要直接跨交易所比较绝对值。不同交易所可能有不同的结算周期、费率上限、指数构成和流动性结构。v0 的正确用法是：**先在同一个交易所内排序，找它自己的极端值。**

## 3. 启动只读观察服务

项目根目录有一个保守的示例：`config.squeeze-radar.example.yaml`。它只观察 BTC、ETH、SOL 在 Binance 与 OKX 的公开现货/永续数据，不使用任何账户凭据。

复制成自己的本地配置：

```powershell
Copy-Item config.squeeze-radar.example.yaml config.squeeze-radar.local.yaml
```

启动：

```powershell
$env:MARKETBRIDGE_CONFIG = "config.squeeze-radar.local.yaml"
cargo run
```

另开一个 PowerShell 窗口确认服务：

```powershell
$mb = "http://127.0.0.1:8080"
Invoke-RestMethod -Uri "$mb/health" | ConvertTo-Json -Depth 6
```

如果你设置了 `MARKETBRIDGE_API_KEY`，每个请求必须加请求头：

```powershell
$headers = @{ "x-api-key" = $env:MARKETBRIDGE_API_KEY }
Invoke-RestMethod -Uri "$mb/health" -Headers $headers
```

后续命令如需认证，均加上 `-Headers $headers`。

## 4. 第一步：全市场 Funding 海选

下面的请求不会启动海量实时订阅；它只是调用支持的公开接口，获得当前 funding 行，用来生成一个**人工审核候选池**。

```powershell
$funding = (Invoke-RestMethod -Uri `
  "$mb/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000").funding

$funding.Count
```

先检查适配器返回的错误，避免把“某交易所请求失败”误当成“该交易所没有异常”：

```powershell
$response = Invoke-RestMethod -Uri `
  "$mb/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000"

$response.errors
```

### 4.1 每家交易所分别找最负的 10 个

```powershell
$funding |
  Group-Object exchange |
  ForEach-Object {
    $_.Group |
      Sort-Object funding_rate_pct |
      Select-Object -First 10 exchange, symbol, funding_rate_pct, mark_price, index_price, next_funding_time_ms
  } |
  Format-Table -AutoSize
```

这一步回答的是：“此时此刻，这家交易所哪些永续合约最偏空？”

它不回答：“哪个币一定会涨？”

### 4.2 用宽阈值建立初始候选池

例如以 `-0.10%` 为初筛阈值：

```powershell
$candidates = $funding |
  Where-Object { $_.funding_rate_pct -le -0.10 } |
  Sort-Object exchange, funding_rate_pct

$candidates |
  Select-Object exchange, symbol, funding_rate_pct, mark_price, index_price, next_funding_time_ms |
  Format-Table -AutoSize
```

不要把 `-0.10%` 当成不变规则。一个合理的工作方式是：

- 大盘与高流动性标的：先观察每家交易所最负的一小部分；
- 小币或异常波动币：先提高流动性审查标准，而不是因为 funding 更负就更信任；
- 某交易所费率长期偏负：用该交易所自身近期分布判断，避免绝对阈值误判。

## 5. 第二步：确认合约与资产身份

例如 funding 海选出现 `LSKUSDT`，先确认该交易所的永续合约目录：

```powershell
$contracts = Invoke-RestMethod -Uri `
  "$mb/v1/catalog/perpetuals?exchange=binance&base=LSK&limit=50000"

$contracts | ConvertTo-Json -Depth 10
```

审核时至少回答：

1. 合约是否 `active`；
2. `symbol`、`native_symbol`、`base`、`quote` 与你想研究的资产是否一致；
3. 是否存在迁移、改名、同 ticker 多资产或不同结算资产；
4. 同一交易所是否有可对应的现货数据；
5. 盘口与成交是否足以让研究结果有意义。

不要按 ticker 自动认为资产相同。MarketBridge 的通用资产关系模型就是为了避免“名字一样，所以可以套利/对冲”的错误。

## 6. 第三步：把少量候选放入实时观察池

资金费率接口可以扫全市场；但 CVD、OI 窗口、盘口 OFI 与清算等实时证据需要被观察的标的持续产生事件。因此把审核后的候选加入配置，而不是将全部 funding 行加入。

编辑 `config.squeeze-radar.local.yaml`：

```yaml
symbols: [BTCUSDT, ETHUSDT, SOLUSDT, LSKUSDT, TRBUSDT]
perp_symbols: [BTCUSDT, ETHUSDT, SOLUSDT, LSKUSDT, TRBUSDT]
```

然后重启服务。建议第一轮控制在 5–20 个标的，原因是：

- 公共接口会有速率与连接限制；
- 极小流动性币的盘口、强平和 OI 数据更容易缺失；
- 太多标的会降低你人工复核候选与归档实验的质量；
- 观察池应来自明确研究假设，而不是无界“扫链式订阅”。

## 7. 预热：为什么刚启动不能马上得出结论

Squeeze Radar v0 不会填补不存在的历史。启动后，各字段会逐渐可用：

| 数据 | 最早有意义的时点 | 原因 |
| --- | --- | --- |
| funding、最新 OI、价格 | 首次相应事件后 | 只是当前快照，不代表趋势。 |
| 1 分钟 CVD / OFI | 收到足够成交与盘口事件后 | 需要主动买卖与盘口变化。 |
| 15 分钟价格变化 | 约 15 分钟后 | 需要一条接近 15 分钟前的对齐基准。 |
| 1 小时 OI 变化 | 约 1 小时后 | 需要一条接近 1 小时前的 OI 基准。 |

窗口不是“随便拿最早一条旧数据”。系统只接受与目标窗口相差不超过容忍范围的基准；基准不足会输出 `null` 并进入 `warming_up`。

服务重启会清空 v0 的内存窗口。长期实验应启用数据湖/录制，并通过已有回放能力完成，而不是依赖人工截图。

## 8. 第四步：调用逼空雷达

扫描 Binance 的已观察标的：

```powershell
$scan = Invoke-RestMethod -Uri `
  "$mb/v1/research/squeeze/scan?exchange=binance&max_data_age_ms=3000&minimum_score=0&limit=50"

$scan.candidates |
  Select-Object exchange, symbol, state, score,
    @{N="Fresh";E={$_.data_quality.fresh}},
    @{N="Ready";E={$_.data_quality.ready_for_trigger}},
    evidence, missing_evidence |
  Format-List
```

字段解释：

| 字段 | 应怎样使用 |
| --- | --- |
| `state` | 先看是否 `warming_up` 或 `stale_data`；这两类不进入实验。 |
| `score` | 研究排序分数，不是仓位大小，也不是收益概率。 |
| `data_quality.fresh` | 当前服务是否收到足够新的整体市场事件。 |
| `data_quality.ready_for_trigger` | funding、OI、价格、盘口及窗口基线是否同时满足最小条件。 |
| `oi_1h_baseline` | 查看 `change_pct` 与 `actual_elapsed_ms`，确认不是误标一小时。 |
| `price_15m_baseline` | 右侧价格确认；仍下跌时不会得到该项加分。 |
| `evidence` | 系统实际用到的证据，而非概括性结论。 |
| `missing_evidence` | 不可忽略的缺口，例如 OI 未预热、盘口过期、CVD 不匹配。 |

## 9. 什么条件才值得归档

最低条件不是“分数高”，而是：

```text
data_quality.fresh = true
data_quality.ready_for_trigger = true
state = triggered_research_candidate 或 armed_research_candidate
missing_evidence 已人工阅读
```

一个理想的 `triggered_research_candidate` 通常同时具有：

```text
负 funding
+ 约 1 小时 OI 持续增加
+ 现货 CVD 为正、永续 CVD 为负
+ 永续盘口 OFI 为正
+ 有近期买方强平
+ 约 15 分钟价格为正，完成右侧确认
```

分数达到 8 也不能替代事实审查。例如某个极小币只有极薄盘口，即便几项指标都正，也可能无法在任何合理规模上成交。

## 10. 归档：让每次判断可以复盘

归档当前扫描结果：

```powershell
$archive = Invoke-RestMethod -Method Post -Uri `
  "$mb/v1/research/squeeze/archive?exchange=binance&max_data_age_ms=3000&minimum_score=0&limit=50"

$archive | ConvertTo-Json -Depth 12
```

返回的 `document.namespace` 为 `squeeze-scans`，`document.id` 是不可变记录标识。归档内容包含：

- 雷达模型版本；
- 原始实时状态；
- funding、OI、价格、CVD、盘口和清算证据；
- 每个固定窗口的时间基准；
- 数据新鲜度与缺口；
- 本次扫描参数与候选排序。

建议建立自己的实验表，至少记录：

| 字段 | 示例 |
| --- | --- |
| `archive_id` | `squeeze-scan-...` |
| 标的 / 交易所 | `LSKUSDT / binance` |
| 触发时间 | 归档的 `generated_at_ms` |
| 拟研究的观察窗口 | 15m、1h、4h |
| 模拟入场规则 | 下一可见 ask，指定最大滑点与规模 |
| 模拟退出规则 | 结构失效、时间退出或后续统计窗口 |
| 成本假设 | 手续费、价差、资金费、部分成交 |
| 结果 | 有利/不利波动、最大回撤、净结果 |

## 11. 当前不能从雷达得出的结论

以下内容在 v0 中会被保留为“缺失”或“未证明”，不能自行补成信号：

- 流通市值、真实 OI/市值比；
- 充值/提现是否全局关闭；
- 借币库存和借贷利率；
- 交易所钱包储备变化；
- 团队钱包、巨鲸或交易所地址的准确归属；
- 持仓集中度、休眠筹码移动；
- 50x/100x 强平墙价格；
- 自动入场、自动止损、自动下单和真实 PnL。

这些能力需要专用、可审计的数据源及单独的身份/覆盖率模型。未实现时，正确做法是让它保持未知，而不是用社交媒体叙事填补。

## 11.1 可选数据层：流通市值与 `OI / Mcap`

MarketBridge 可以从 CoinGecko 获取供应参考快照，但默认关闭。系统不会根据
`LSKUSDT` 的字符串自动猜测 CoinGecko 的 `lisk` 条目；必须在配置里提供稳定
`asset_id`、供应商 ID、明确绑定的永续 symbol，以及身份依据。

```yaml
reference_data:
  supply:
    enabled: true
    provider: coingecko
    poll_secs: 60
    assets:
      - asset_id: "lisk-v2"
        provider_asset_id: "lisk"
        perp_symbols: [LSKUSDT]
        identity_evidence: "记录迁移/合约/资产映射依据的 URL 或内部文档"
        chain: "ethereum"
```

查看快照：

```powershell
Invoke-RestMethod -Uri "$mb/v1/reference/supply?perp_symbol=LSKUSDT" |
  ConvertTo-Json -Depth 12
```

当且仅当 OI 是可用 USD 名义值、供应快照未过期、且显式身份映射存在时，雷达
才返回 `supply_context.oi_to_circulating_mcap`。这是供应商报告的流通市值参考，
不是自由流通盘、可成交容量或收益保证。

## 11.2 可选数据层：充提与网络维护状态

MarketBridge 不接收用户的交易、提现或钱包权限。若有独立的公开公告采集器，或
隔离的只读账户观察器，可将标准化证据写入本地 API：

```powershell
$body = @{
  venue = "binance"
  asset_id = "lisk-v2"
  operation = "deposit"
  status = "disabled"
  scope = "public_announcement_observed"
  observed_at_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
  source_kind = "venue_announcement"
  source_url = "https://example.com/public-announcement"
  network = "ethereum"
  note = "保留原始公告链接和网络范围"
} | ConvertTo-Json

Invoke-RestMethod -Method Post -ContentType "application/json" `
  -Uri "$mb/v1/reference/venue-asset-status" -Body $body |
  ConvertTo-Json -Depth 8
```

读取当前状态：

```powershell
Invoke-RestMethod -Uri "$mb/v1/reference/venue-asset-status?venue=binance&asset_id=lisk-v2" |
  ConvertTo-Json -Depth 10
```

`scope=account_observed` 表示一个账户观察到的状态，不能解释为全局停充；只有
`scope=public_announcement_observed` 且来源链接可复核时，才可作为公开事件证据。

## 12. 一个每日研究节奏

一个保守、可重复的节奏可以是：

1. 每 5–15 分钟调用一次全市场 funding 海选；
2. 按交易所分别查看最负的若干标的；
3. 审核合约身份、流动性和现货/永续映射；
4. 只把通过审核的少量标的放入观察配置；
5. 预热到 OI 1h 与价格 15m 都有基准；
6. 每 15–60 秒扫描一次实时雷达；
7. 只有数据质量合格的 `armed` 或 `triggered` 候选才归档；
8. 每周汇总全部归档记录，连同失败样本一起做纸面统计。

不要只保存最后上涨的币。未触发、触发失败、触发后继续下跌、因数据缺失被拒绝的样本，都是判断这条策略是否真的有效所必需的数据。

## 13. 排障清单

| 现象 | 优先检查 |
| --- | --- |
| funding 返回为空 | 查看 `errors`；确认交易所、quote 与网络状态。 |
| 雷达没有候选 | 检查当前配置是否包含 `perp_symbols`，以及进程是否已收到行情。 |
| 全部是 `warming_up` | 正常；等待 15 分钟价格基准和 1 小时 OI 基准。 |
| 全部是 `stale_data` | 检查网络、交易所连接、进程日志与 `max_data_age_ms`。 |
| `Ready = false` | 阅读 `missing_evidence`，不要只看总分。 |
| 一重启结果消失 | v0 窗口在内存；使用数据湖/录制与回放保存长期实验。 |
| 某币看起来极端却没有触发 | 可能缺盘口、现货成交、OI 基准或右侧价格确认；这正是雷达的保护机制。 |

## 14. 结论

MarketBridge 在这条策略中的作用不是替你预测“下一只百倍币”，而是将模糊叙事拆成可观察、可拒绝、可归档的市场条件。先把负 funding 当作候选发现器，再用 OI、CVD、订单簿、清算和价格确认过滤，最后用完整样本验证。只有这样，逼空策略才有机会从事后截图走向可复现研究。
