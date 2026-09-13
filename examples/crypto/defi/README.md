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

Provenance: the decomposition follows the public [Uniswap explanation of pool
liquidity and price impact](https://developers.uniswap.org/docs/get-started/concepts/how-uniswap-works)
and [swap execution](https://developers.uniswap.org/docs/get-started/concepts/traders/swaps).
MarketBridge currently uses bounded provider snapshots and does not claim a
complete on-chain swap ledger or protocol-native route depth.

## 中文

这一系列使用 MarketBridge 标准化的 DEX 池报价和 `defi_native_state` 信号，检验一个窄而可证伪的问题：
相对于报告流动性，单小时 swap 交易量很高的池子，是否会持续处于更高的流动性压力状态？监控会报告
`high_turnover_pool` 和 `thin_liquidity_high_flow`，同时保留买卖笔数和报价新鲜度。

`crypto_defi_pool_flow_recorder.py` 先冻结连续快照，`crypto_defi_pool_flow_replay.py` 再用 `--min-run`
检验压力状态是否持续。这是执行风险和池状态诊断，不是 LP APR、无常损失、手续费、路由、MEV 或钱包策略。

出处：拆解参考 [Uniswap 关于流动性池和价格冲击的说明](https://developers.uniswap.org/docs/get-started/concepts/how-uniswap-works)
以及 [swap 执行说明](https://developers.uniswap.org/docs/get-started/concepts/traders/swaps)。MarketBridge 当前使用有界的
提供方快照，不声称完整覆盖链上 swap ledger 或协议原生路由深度。

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
```
