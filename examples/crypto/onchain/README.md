# On-chain transfer pressure / 链上转账压力

## Bitcoin mempool pressure / 比特币 mempool 费率压力

### English

`crypto_onchain_mempool_pressure_monitor.py` classifies the current
`/v1/onchain/mempool` snapshot as `high_fee_pressure`, `low_fee_pressure`, or
`ordinary_fee_pressure` using recommended sat/vB and mempool virtual-size
thresholds. The recorder freezes that state beside a synchronized BTC quote;
the replay compares fixed-record signed and absolute BTC responses by state.

This is a network-congestion context study, not a fee-selection tool or a
directional BTC signal. A high fee rate can reflect demand for block space,
application activity, spam, or provider-specific node state. The study does not
infer cause, transaction identity, wallet ownership, or confirmation time.

Provenance: [Alex Thorn's public X observation about an empty Bitcoin mempool](https://x.com/intangiblecoins/status/2043001350628184497)
is treated only as a regime-context claim. The [official mempool.space REST API](https://mempool.space/docs/api/rest)
defines the `/api/mempool`, `/api/v1/fees/recommended`, and tip-height inputs;
its [FAQ](https://mempool.space/docs/faq) explains that fee suggestions are
guidance and do not guarantee confirmation timing.

### 中文

`crypto_onchain_mempool_pressure_monitor.py` 读取 `/v1/onchain/mempool`，用
推荐 sat/vB 费率和 mempool 虚拟大小把快照分类为高费率压力、低费率压力或普通压力。
recorder 会把状态与同步 BTC 报价冻结到 JSONL；replay 再按状态比较固定记录窗口的
BTC 有符号/绝对响应。

这是网络拥堵上下文研究，不是选手续费工具，也不是 BTC 方向信号。高费率可能来自区块
空间需求、应用活动、垃圾交易或 provider 节点差异；本案例不推断原因、交易身份、钱包
归属或确认时间。

出处：只把 [Alex Thorn 在 X 上关于 Bitcoin mempool 为空的公开观察](https://x.com/intangiblecoins/status/2043001350628184497)
作为状态线索；输入字段以 [mempool.space 官方 REST API](https://mempool.space/docs/api/rest)
和其[官方 FAQ](https://mempool.space/docs/faq)为准。FAQ 明确说明推荐费率是参考值，
不保证确认时间。

## Bitcoin mining pressure / 比特币矿工压力

### English

`crypto_onchain_mining_pressure_monitor.py` reads `/v1/onchain/mining` and
classifies `miner_stress_context`, `miner_tailwind_context`, or
`ordinary_mining_context` from the current difficulty adjustment and the
provider's seven-day hashrate change. The recorder freezes the state beside a
BTC quote; the replay compares later signed and absolute BTC responses by state.

This does not turn a negative difficulty change into a miner-capitulation claim.
Hashrate is estimated, difficulty is a protocol adjustment, and neither field
identifies a miner's reserve, profitability, treasury sale, or forced selling.

Provenance: [CryptoDiffer's public X discussion of an 11.16% Bitcoin difficulty drop and miner-capitulation interpretation](https://x.com/CryptoDiffer/status/2021059510106980651)
is treated as an unverified research lead. The inputs are cross-checked against
the [official mempool.space REST API](https://mempool.space/docs/api/rest), which
documents difficulty-adjustment and hashrate endpoints.

### 中文

`crypto_onchain_mining_pressure_monitor.py` 读取 `/v1/onchain/mining`，根据当前难度
调整和 provider 七日哈希率变化，分类为矿工压力、矿工顺风或普通网络状态。recorder
会把状态与 BTC 报价冻结；replay 按状态比较之后固定记录窗口的有符号/绝对响应。

负难度调整不等于已经证明矿工投降。哈希率是估计值，难度是协议调整；二者都不能识别
矿工储备、盈利、财库出售或被迫卖出。

出处：[CryptoDiffer 在 X 上关于 Bitcoin 难度下调 11.16% 与 miner capitulation 的公开讨论](https://x.com/CryptoDiffer/status/2021059510106980651)
只作为未验证研究线索；字段以 [mempool.space 官方 REST API](https://mempool.space/docs/api/rest)
中的 difficulty-adjustment 与 hashrate 接口为准。

## English

This family tests a deliberately conservative stablecoin/large-transfer lead:
when the public transfer cache accumulates a large USD notional in a rolling
window, is the following absolute price movement larger than ordinary windows?
The result is non-directional. `direction`, asset, chain and address fields are
kept as evidence only; they do not prove exchange inflow, selling pressure or
net stablecoin supply.

`crypto_onchain_transfer_burst_replay.py` uses
`/v1/onchain/transfers` plus `/v1/history/candles`, compares burst windows with
all ordinary forward windows, and requires a minimum observation count. It is
a bounded public-data replay, not a flow-trading, wallet or execution strategy.

`crypto_onchain_transfer_response_recorder.py` freezes the transfer feed beside
a synchronized MarketBridge BTC quote. Its paired replay reconstructs the
rolling transfer window from the JSONL archive, deduplicates repeated provider
rows using observable fields, and compares burst versus ordinary windows at a
fixed record horizon. The result is non-directional and remains observe-only
when quote coverage or transfer coverage is insufficient.

Provenance: the research lead comes from [a public stablecoin-liquidity post on
X](https://x.com/Cointelegraph/status/2029519994652942494). The measurement
boundary is tightened by the [BIS transfer-level stablecoin study](https://www.bis.org/publications/working-paper-1359-anatomy-stablecoin-transactions),
which warns that individual transfers can be part of bundled trading, lending,
arbitrage or settlement activity. MarketBridge therefore tests only an
absolute-move association and keeps coverage limitations visible.

## 中文

这一系列把“稳定币/大额转账代表流动性回流”的线索收窄成保守、可证伪的问题：滚动窗口内公开转账
累计美元名义金额很大时，之后固定 K 线窗口的绝对波动是否高于普通窗口？结果不带方向。`direction`、
资产、链和地址字段只作为证据保留，不证明交易所净流入、卖压或稳定币净增发。

`crypto_onchain_transfer_burst_replay.py` 使用 `/v1/onchain/transfers` 和 `/v1/history/candles`，把
转账 burst 窗口与所有普通前瞻窗口比较，并要求最小样本数。这是有界公开数据回放，不是资金流交易、
钱包或执行策略。

`crypto_onchain_transfer_response_recorder.py` 会把转账 feed 与同步的 MarketBridge BTC 报价冻结到 JSONL；配套
replay 在归档中重建滚动转账窗口，用可观察字段去重 provider 重复行，并在固定记录窗口比较 burst 与普通窗口。
结果保持非方向性；报价或转账覆盖不足时只输出 observe-only。

出处：研究线索来自[公开 X 稳定币流动性讨论](https://x.com/Cointelegraph/status/2029519994652942494)。
测量边界参考 [BIS 关于稳定币转账级数据的研究](https://www.bis.org/publications/working-paper-1359-anatomy-stablecoin-transactions)，
该研究提醒单笔转账可能属于捆绑交易、借贷、套利或结算。MarketBridge 因此只检验绝对波动关联，
并明确保留覆盖限制。

数据接口边界同时对照 [Whale Alert Alerts API 文档](https://developer.whale-alert.io/api-account/documentation)：
转账包含时间、资产和 USD value，但地址归属和 provider 阈值不等于交易所净流入。这里不把公开转账方向解释成卖压。

## Commands / 命令

```bash
python3 examples/crypto/onchain/crypto_onchain_transfer_burst_replay.py \
  --source whale_alert --asset USDT --min-transfer-usd 100000 \
  --price-exchange binance --symbol BTCUSDT --interval 5m \
  --threshold-usd 1000000 --window-hours 24 --horizon-bars 12
python3 examples/crypto/onchain/crypto_onchain_transfer_response_recorder.py \
  --source whale_alert --asset USDT --min-transfer-usd 100000 \
  --price-exchange binance --price-symbol BTCUSDT \
  --iterations 120 --interval-secs 60 \
  --output work/crypto-onchain-transfer-response.jsonl
python3 examples/crypto/onchain/crypto_onchain_transfer_response_replay.py \
  --input work/crypto-onchain-transfer-response.jsonl \
  --window-hours 24 --horizon-records 12 \
  --threshold-usd 1000000 --min-observations 3

python3 examples/crypto/onchain/crypto_onchain_mempool_pressure_monitor.py \
  --high-fee-sat-vb 20 --low-fee-sat-vb 3 \
  --high-vsize-mb 150 --low-vsize-mb 25
python3 examples/crypto/onchain/crypto_onchain_mempool_pressure_recorder.py \
  --price-exchange binance --price-symbol BTCUSDT \
  --iterations 120 --interval-secs 60 \
  --output work/crypto-onchain-mempool-pressure.jsonl
python3 examples/crypto/onchain/crypto_onchain_mempool_pressure_replay.py \
  --input work/crypto-onchain-mempool-pressure.jsonl \
  --horizon-records 12 --min-observations 3
python3 examples/crypto/onchain/crypto_onchain_mining_pressure_monitor.py \
  --stress-difficulty-pct -3 --stress-hashrate-pct -3
python3 examples/crypto/onchain/crypto_onchain_mining_pressure_recorder.py \
  --price-exchange binance --price-symbol BTCUSDT \
  --iterations 30 --interval-secs 600 \
  --output work/crypto-onchain-mining-pressure.jsonl
python3 examples/crypto/onchain/crypto_onchain_mining_pressure_replay.py \
  --input work/crypto-onchain-mining-pressure.jsonl \
  --horizon-records 12 --min-observations 3
```
