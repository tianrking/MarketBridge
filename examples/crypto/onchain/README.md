# On-chain transfer pressure / 链上转账压力

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
```
