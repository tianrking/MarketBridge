# DeFi pool flow and liquidity / DeFi 池流量与流动性

## English

This family uses MarketBridge's normalized DEX-pool quote and
`defi_native_state` signals. It asks a narrow, falsifiable question: does a
pool with high one-hour swap volume relative to reported liquidity persist as a
higher-pressure state? The monitor reports `high_turnover_pool` and
`thin_liquidity_high_flow`, while retaining buy/sell counts and quote freshness.

`crypto_defi_pool_flow_recorder.py` freezes repeated snapshots and
`crypto_defi_pool_flow_replay.py` tests whether the pressure state persists for
`--min-run` observations. This is an execution-risk and pool-regime diagnostic;
it is not an LP APR, impermanent-loss, fee, route, MEV, or wallet strategy.

`crypto_defi_pool_flow_response_recorder.py` adds a synchronized MarketBridge
BTC quote to each pool snapshot. Its paired replay compares the later BTC
signed and absolute return after `pressure` (thin liquidity/high flow or high
turnover) versus `ordinary_pool_activity` snapshots. The record-count horizon
is explicit: this is a descriptive event study, not proof that pool pressure
causes BTC movement or that a swap is executable.

`crypto_stablecoin_depeg_monitor.py` / `crypto_stablecoin_depeg_recorder.py` /
`crypto_stablecoin_depeg_replay.py` observe selected stablecoin pair deviations
and compare stressed snapshots with later absolute BTC movement. This is a
risk event study, not a depeg-arbitrage or liquidity-withdrawal instruction.

`crypto_stablecoin_rotation_response_replay.py` reuses that JSONL archive for a
narrow directional hypothesis: after normalizing either `USDCUSDT` or the
inverse `USDTUSDC` quote into USDC priced in USDT, does a USDC discount align
with positive BTC movement and a premium with negative movement? It reports
aligned-return statistics only; it does not infer capital flows, redemption
pressure or an executable conversion.

`crypto_jupiter_route_impact_monitor.py` / `crypto_jupiter_route_impact_recorder.py` /
`crypto_jupiter_route_impact_replay.py` consume Jupiter's read-only quote
diagnostics from `defi_native_state`. When `defi.jupiter.pairs[].route_amounts`
is configured, each input-size ladder point is kept separate and the case
tests whether router-reported price impact and route-hop states persist at
larger sizes. The replay is a route-observation study: it does not claim full
pool depth, gas, MEV, fill probability, wallet access, or swap execution.
Enable the Jupiter source and add a ladder in `config.yaml`, for example:

```yaml
defi:
  jupiter:
    enabled: true
    pairs:
      - symbol: SOLUSDC
        amount: 1000000000
        route_amounts: [5000000000, 10000000000]
```

Provenance: the decomposition follows the public [Uniswap explanation of pool
liquidity and price impact](https://developers.uniswap.org/docs/get-started/concepts/how-uniswap-works)
and [swap execution](https://developers.uniswap.org/docs/get-started/concepts/traders/swaps).
MarketBridge currently uses bounded provider snapshots and does not claim a
complete on-chain swap ledger or protocol-native route depth.

The Jupiter case uses the official [Jupiter quote API documentation](https://developers.jup.ag/docs/swap/v1/get-quote),
which documents `priceImpactPct` and `routePlan` in a quote response. Those
fields are evidence for a bounded route-impact hypothesis, not an executable
price guarantee.

The stablecoin case follows the unverified [DEWS-style early-warning discussion
on X](https://x.com/crazydnekana/status/2030633787462242588), which describes
price drift, thinning liquidity and trading pressure as a sequence to monitor.
It is cross-checked against the peer-reviewed [Tether depegging and crypto
returns study](https://doi.org/10.1111/acfi.70201) and the research [Detecting
Depegs paper](https://arxiv.org/abs/2306.10612). MarketBridge tests only quote
deviation and subsequent absolute movement; it does not infer reserves,
redemptions, solvency or executable mean reversion.

## 中文

这一系列使用 MarketBridge 标准化的 DEX 池报价和 `defi_native_state` 信号，检验一个窄而可证伪的问题：
相对于报告流动性，单小时 swap 交易量很高的池子，是否会持续处于更高的流动性压力状态？监控会报告
`high_turnover_pool` 和 `thin_liquidity_high_flow`，同时保留买卖笔数和报价新鲜度。

`crypto_defi_pool_flow_recorder.py` 先冻结连续快照，`crypto_defi_pool_flow_replay.py` 再用 `--min-run`
检验压力状态是否持续。这是执行风险和池状态诊断，不是 LP APR、无常损失、手续费、路由、MEV 或钱包策略。

`crypto_defi_pool_flow_response_recorder.py` 会在每个池状态快照旁边记录同步的 MarketBridge BTC 报价；配套 replay
比较 `pressure`（薄流动性高流量或高换手）与 `ordinary_pool_activity` 快照之后的 BTC 有符号和绝对收益。
回放窗口按记录数明确给出，只是描述性事件研究，不证明池压力造成 BTC 变动，也不代表 swap 可执行。

出处：拆解参考 [Uniswap 关于流动性池和价格冲击的说明](https://developers.uniswap.org/docs/get-started/concepts/how-uniswap-works)
以及 [swap 执行说明](https://developers.uniswap.org/docs/get-started/concepts/traders/swaps)。MarketBridge 当前使用有界的
提供方快照，不声称完整覆盖链上 swap ledger 或协议原生路由深度。

稳定币案例参考未经验证的 [X 上 DEWS 风险预警讨论](https://x.com/crazydnekana/status/2030633787462242588)，
其提出持续价格漂移、流动性变薄和交易压力的监控顺序；并对照同行评审的
[Tether 脱锚与加密资产收益研究](https://doi.org/10.1111/acfi.70201) 以及
[Detecting Depegs 研究](https://arxiv.org/abs/2306.10612)。MarketBridge 只检验报价偏离和之后的绝对波动，
不推断储备、赎回、偿付能力或可执行均值回归。

`crypto_stablecoin_rotation_response_replay.py` 复用该 JSONL 归档，专门检验一个有方向的窄假设：将 `USDCUSDT` 或反向
`USDTUSDC` 归一化为“USDC 以 USDT 计价”后，USDC 折价是否与 BTC 上涨、USDC 溢价是否与 BTC 下跌对齐？输出只包含
方向对齐收益统计，不推断资金流、赎回压力或可执行兑换。

`crypto_jupiter_route_impact_monitor.py` / `crypto_jupiter_route_impact_recorder.py` /
`crypto_jupiter_route_impact_replay.py` 消费 `defi_native_state` 中 Jupiter 的只读报价诊断。
在 `defi.jupiter.pairs[].route_amounts` 配置输入量梯度后，每个规模会独立保留，回放检验较大输入量下
路由器报告的 price impact 与跳数状态是否持续。它只是路由观察研究，不声称完整池深度、gas、MEV、成交概率、钱包权限或 swap 执行。
先在 `config.yaml` 启用 Jupiter，并在对应 pair 添加例如 `route_amounts: [5000000000, 10000000000]`；主 `amount`
仍会始终查询。这样可以清楚区分 1、5、10 SOL 等输入规模（原子单位），但不会自动扩展成完整池深度。

出处：Jupiter 官方 [Quote API 文档](https://developers.jup.ag/docs/swap/v1/get-quote) 明确记录 quote 返回的
`priceImpactPct` 与 `routePlan`。这些字段只能支撑有界的路由冲击假设，不能当作可执行价格保证。

## Commands / 命令

```bash
python3 examples/crypto/defi/crypto_defi_pool_flow_monitor.py \
  --sources uniswap_v3,meteora --min-liquidity-usd 100000 \
  --min-turnover-h1 0.25
python3 examples/crypto/defi/crypto_defi_pool_flow_recorder.py \
  --sources uniswap_v3,meteora --iterations 30 --interval-secs 30 \
  --output work/crypto-defi-pool-flow.jsonl
python3 examples/crypto/defi/crypto_defi_pool_flow_replay.py \
  --input work/crypto-defi-pool-flow.jsonl --min-run 3
python3 examples/crypto/defi/crypto_defi_pool_flow_response_recorder.py \
  --sources uniswap_v3,meteora --price-exchange binance --price-symbol BTCUSDT \
  --iterations 120 --interval-secs 30 \
  --output work/crypto-defi-pool-flow-response.jsonl
python3 examples/crypto/defi/crypto_defi_pool_flow_response_replay.py \
  --input work/crypto-defi-pool-flow-response.jsonl \
  --horizon-records 3 --min-observations 10
python3 examples/crypto/defi/crypto_jupiter_route_impact_monitor.py \
  --symbols SOLUSDC --min-impact-ratio 0.005
python3 examples/crypto/defi/crypto_jupiter_route_impact_recorder.py \
  --symbols SOLUSDC --iterations 30 --interval-secs 30 \
  --output work/crypto-jupiter-route-impact.jsonl
python3 examples/crypto/defi/crypto_jupiter_route_impact_replay.py \
  --input work/crypto-jupiter-route-impact.jsonl --min-run 3
python3 examples/crypto/defi/crypto_stablecoin_depeg_monitor.py \
  --exchange binance --stable-symbols USDTUSDC,USDCUSDT,DAIUSDT \
  --risk-symbol BTCUSDT --watch-bps 20 --stress-bps 50
python3 examples/crypto/defi/crypto_stablecoin_depeg_recorder.py \
  --exchange binance --iterations 120 --interval-secs 30 \
  --output work/crypto-stablecoin-depeg.jsonl
python3 examples/crypto/defi/crypto_stablecoin_depeg_replay.py \
  --input work/crypto-stablecoin-depeg.jsonl --horizon-snapshots 3 \
  --stress-bps 50 --min-stress 3 --min-ordinary 3
python3 examples/crypto/defi/crypto_stablecoin_rotation_response_replay.py \
  --input work/crypto-stablecoin-depeg.jsonl --horizon-snapshots 3 \
  --threshold-bps 5 --min-observations 5
```
