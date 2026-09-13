# 12 · 将外部策略观点变成可验证实验

X 上的策略帖适合用作**假设来源**，不应直接变成交易规则。MarketBridge 的角色是把
一个观点拆成数据条件、失效条件和可重放的实验；它不下单，也不以评分代替收益证明。

## 1. 先做三段式筛选

对每个外部观点记录来源链接、首次观察时间和原始主张，并只在同时满足下列条件时进入
观察池：

1. 能指出可测量的输入，而不是只给出图形或结果截图；
2. 输入可以由 MarketBridge 当前数据、明确的新数据源，或人工导入取得；
3. 有可证伪条件、目标持有窗口和成本／流动性假设。

没有满足这些条件的内容可保留在“叙事”栏，但不能被扫描器、告警或纸面账本采用。

## 2. 常见观点与当前覆盖

| 外部观点 | 可验证的最小假设 | MarketBridge 输入 | 当前成熟度 |
| --- | --- | --- | --- |
| 负 funding 预示空头挤压 | 负 funding 持续，OI 增加，现货买压和价格确认同时出现 | funding、OI、spot/perp CVD、OFI、清算、klines | 可直接验证；使用 squeeze radar |
| 高 funding 后多头耗竭 | 正 funding、未能站稳新高、OI 回落、perp 卖压和薄买盘共振 | funding、OI、klines、order flow、L2、链上转账（可选） | 可直接验证；使用 exhaustion 示例 |
| 现货—永续价差／资金费差 | 以同资产、同规模、明确费用后的净差为正 | quotes/books、basis、funding、研究成本曲线 | 可直接做研究；不是可执行套利承诺 |
| 清算瀑布或“liquidation wall” | 清算方向、OI 变化和成交／盘口在定义窗口内共振 | 原生 liquidation、OI、trades/books；聚合源可选 | 部分；热力图墙需要可审计数据源 |
| 大额链上流入交易所 | 地址标签可靠，转账在预设窗口内改善模型的样本外结果 | Whale Alert / Etherscan、quotes、klines | 部分；标签和因果都必须独立验证 |
| 社媒情绪抢跑 | 可重放的帖子时间、资产实体识别和反垃圾规则改善样本外结果 | 当前仅外部公告／情绪输入 | 需要新数据源；不可从截图推导 |

## 3. 两个可运行的观察示例

先以 `config.squeeze-radar.example.yaml` 启动只读行情服务，并让它预热：

```bash
cp config.squeeze-radar.example.yaml config.squeeze-radar.local.yaml
export MARKETBRIDGE_CONFIG=./config.squeeze-radar.local.yaml
cargo run
```

在另一终端运行空头拥挤／逼空观察（默认 30 秒轮询；`--iterations` 便于做短验收）：

```bash
cargo run --example short_squeeze_monitor -- \
  --symbol BTCUSDT --exchange binance --interval-secs 30 --iterations 3
```

运行多头拥挤／耗竭观察：

```bash
cargo run --example exhaustion_short_monitor -- \
  --symbol BTCUSDT --exchange binance --interval-secs 30 --iterations 3
```

X 上常见的另一类观点是「现货多、永续空」的 delta-neutral carry：正 basis 加上正
funding 时，空永续的一侧可能收到 funding。MarketBridge 已有 basis 与 funding 输入，
对应的研究示例是：

```bash
cargo run --example basis_carry_monitor -- \
  --symbol BTCUSDT --exchange binance --interval-secs 30 --iterations 3
```

这个 demo 只在接口提供 `funding_interval_ms` 时换算每日 funding proxy；否则主动拒绝
年化。它不会把永续 basis 当成必然收敛，也不会推断借币、保证金、手续费或成交容量。
生产研究必须把 venue 实际结算周期、借贷成本、执行成本和退出条件加入 paper/replay 输入。

清算反转案例使用 `/v1/market/liquidations`、`/v1/market/open-interest`、
`/v1/market/order-flow` 和 5 分钟 K 线：

```bash
cargo run --example liquidation_reversal_monitor -- \
  --symbol BTCUSDT --exchange binance --interval-secs 30 --iterations 3
```

它将卖方清算视为“多头被迫卖出”的待验证代理，再要求 OI 下降、CVD 转正和价格恢复。
不同 venue 的 liquidation `side` 语义必须用 fixture 或原始文档确认，不能跨交易所直接
合并名义金额。

预测市场的互补结果监控也可直接运行：

```bash
python3 examples/polymarket_complement_monitor.py \
  --min-edge-bps 10 --min-ask-depth 1
```

它读取 Gamma 市场 metadata 和 CLOB YES/NO best ask，寻找扣除缓冲后的价格和低于 1
的候选。`ask_depth` 只是当前快照深度，不是可成交容量；要验证 X 上的回测主张，还需要
逐笔 fills、队列位置、费用、延迟和结算生命周期数据。

公开成交历史现在也可通过 MarketBridge 查询：

```bash
curl -s "http://127.0.0.1:8080/v1/prediction/trades?market=0x...&limit=1000&taker_only=false" | jq
python3 examples/prediction_trade_flow.py --market 0x... --limit 1000
```

该接口接入的是公开 Data API 观察记录，不是需要签名的用户账本，也不声称包含完整成交
队列。它已经足以建立逐笔时间轴、方向流和 VWAP 的研究输入；下一步再将这些记录接到
有费用与结算标签的 replay。上游字段和筛选约定见 [Polymarket trades API](https://docs.polymarket.com/api-reference/core/get-trades-for-a-user-or-markets)，
请求频率也必须遵守 [官方 rate limits](https://docs.polymarket.com/api-reference/rate-limits)。

已结算市场的最小纸面回放：

```bash
python3 examples/polymarket_settlement_replay.py \
  --market 0x... --max-entry-price 0.80 --fee-bps 30
```

回放默认从 `/polymarket/markets?include_closed=true&max_offset=0` 取得第一批已结算市场的
`resolved_outcome`；需要更深分页时显式传 `--market-max-offset`，再从
`/v1/prediction/trades` 取 BUY 记录，按价格上限估算结算 payout 与 paper PnL。它用于
否定或支持假设，不代表完整成交回测；队列、延迟、滑点、争议解决和资金占用时间仍是
下一阶段 feature。

这些 examples 都只输出证据、研究评分或 paper 结果，不会下单。首个轮询没有 OI 变化
基线属正常现象；缺少 L2、成交或链上数据时应将输出视为“不足证据”，而不是零值或反向
信号。

## 4. 从候选到归档

1. 用 `/v1/market/perpetual-funding` 在**单一交易所内**排序，发现候选；
2. 核对合约身份、现货对应关系、盘口深度和数据新鲜度；
3. 将少量候选加入 `perp_symbols`／`symbols`，预热完整窗口；
4. 归档 scanner 或 squeeze scan 的原始证据；
5. 以固定进入、退出、费用和样本外区间做 paper/replay 实验；
6. 只有在失败样本、成本和容量都被保留时，才比较模型版本。

更完整的逼空流程见 [11 · Squeeze Radar](11-squeeze-radar.md)，通用回放和纸面验证见
[05 · History](05-history.md) 与 [06 · Portfolio](06-portfolio.md)。

## 5. 开发优先级

下一批外部策略应优先补“证据可重复性”，而不是增加更多分数：

- 跨 venue 统一的 OI／清算时间序列与数据质量字段；
- 可审计的社媒／公告采集、实体解析、去重和来源版本；
- 按策略版本记录的前瞻标签、费用、滑点和容量；
- 用完整样本和样本外回放报告否定结果。

不要把社媒监控直接接到下单逻辑，也不要因单个 KOL、单张 PnL 图或单次回测而改变阈值。
要冻结一次样本，先把有限页数写入 JSONL：

```bash
python3 examples/polymarket_trade_recorder.py \
  --market 0x... --page-size 1000 --pages 3 \
  --output work/polymarket-trades.jsonl
python3 examples/polymarket_settlement_replay.py \
  --market 0x... --trades-jsonl work/polymarket-trades.jsonl
```

recorder 会拒绝覆盖已有文件、限制页数和 offset，并按 transaction hash（缺失时按
时间／asset／side／size／price／wallet 组合）去重。这样阈值和费用实验可以使用同一份
观测样本，避免上游数据变化造成不可解释的结果漂移。

同一份样本还可以直接做 calibration 检查：

```bash
python3 examples/polymarket_calibration_report.py \
  --trades-jsonl work/polymarket-trades.jsonl --resolved-outcome No
```

它按价格区间输出隐含概率、实际胜率、Brier score 和 log loss，用来检验“高概率买入”
是否真的经过结算校准；它不会因为单一市场的高胜率而宣称存在 alpha。

天气市场的第一步是固定外部观测／预报输入：

```bash
curl -s "http://127.0.0.1:8080/v1/external/weather?latitude=52.52&longitude=13.41&mode=forecast&forecast_days=7&timezone=UTC" | jq
python3 examples/weather_event_observer.py \
  --latitude 52.52 --longitude 13.41 --date 2026-09-14 \
  --min-temp 15 --max-temp 25
python3 examples/weather_pressure_differential.py \
  --market-query "Berlin temperature" \
  --latitude 52.52 --longitude 13.41 --date 2026-09-14 \
  --min-temp 15 --max-temp 25 --max-yes-ask 0.25
```

`/v1/external/weather` 同时支持 `mode=archive&start_date=YYYY-MM-DD&end_date=YYYY-MM-DD`。
它提供可复现的天气输入，不自动推断市场概率；地点映射、bucket 定义、发布时间相对
信息集和 resolution rule 仍需单独归档。

当市场已经结算时，用 manifest 把身份和 resolution rule 固定下来，再做天气一致性与
市场价格的描述性校准：

```bash
python3 examples/weather_market_calibration.py \
  --manifest examples/weather-market-manifest.example.jsonl
```

manifest 必须由调用者提供已验证的 `market_id`、坐标、日期、温度 bucket、
`resolved_outcome`，可选 `yes_price`；脚本不会从 question 文本猜地点或结算规则。
它输出天气 bucket 与结算结果的一致率，以及市场价格的 Brier/log loss，但不把天气模型
当成概率，也不构成交易建议。

## Python-first 策略层

量化研究者不需要修改 Rust。Rust 进程负责数据接入、标准化、缓存、历史和 API；策略
用 Python 调用这些稳定接口：

```bash
python3 examples/python_strategy_runner.py \
  --strategy squeeze --symbol BTCUSDT --exchange binance --iterations 3
```

可选策略为 `squeeze`、`exhaustion`、`basis`、`liquidation`。输出是 JSON 证据和研究
评分，不是交易指令。复制这个文件增加新策略时，应保留输入、缺失数据、成本假设和
`research_only_no_orders` 边界。

公开讨论中常见的「固定时段 + VWAP/EMA/MACD/成交量确认」也可以先做成可证伪过滤器，
而不是把时段本身当成 alpha：

```bash
python3 examples/crypto_session_filter.py \
  --exchange binance --market perp --symbol BTCUSDT \
  --interval 1m --limit 60 --timezone America/New_York \
  --session-start 09:00 --session-end 09:15
```

它只读取 `/v1/market/klines`，输出最新 bar 的 VWAP、EMA、MACD、成交量和时段状态；
需要在不同日期、不同市场状态和样本外区间验证，不能直接转换为 Polymarket 或现货的
下单信号。

跨交易所 funding 也先做差异监控，不直接把 APR 当成收益：

```bash
python3 examples/funding_convergence_monitor.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit \
  --iterations 3 --interval-secs 30
```

脚本只在两个 venue 都有明确 `funding_interval_ms` 时计算每小时差异和年化代理；
任一 provider 没有明确结算间隔，就保留原始行并暂停年化，避免把不同周期的 funding
rate 错误相减。费用、借贷、保证金、转账延迟、滑点、标记价格差异和强平风险都仍是
独立研究变量。

要检验差异是否持续，而不是只看当前 snapshot，可回放公开 funding history：

```bash
python3 examples/funding_convergence_replay.py \
  --symbol BTCUSDT --exchanges binance,bybit \
  --days 7 --limit 200
```

回放会按时间对齐各 venue 最近一笔已知 funding，缺口不会填零；结果只报告观察次数、
超过阈值的比例、中位差异和最大差异。历史 schedule 变化、成交可行性和真实对冲成本
由 history response 的 `funding_schedule.points[]` 提供；成交可行性和真实对冲成本仍需
另外建模。

Liquidation reversal 目前先提供一个边界清晰的 OKX/CoinEx partial replay：

```bash
python3 examples/liquidation_reversal_replay.py \
  --exchange okx --symbol BTCUSDT --limit 100 \
  --horizon-bars 3 --min-notional 100000 \
  --oi-exchange bybit --trades-exchange okx
```

它把 OKX sell-side liquidation 当作 long-liquidation proxy，观察之后三个 5m bar 的
价格变化；CoinEx 事件可用 `--price-exchange okx` 或 `--price-exchange binance` 显式指定价格
上下文，并可选地加入 Binance/Bybit 的公开历史 OI 变化。由于 OI 可能来自不同 venue，以及
完整跨 venue liquidation ledger 与成交成本仍缺失，所以结果不能解释成完整策略回测。
