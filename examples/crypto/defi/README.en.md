# Crypto DeFi flow and liquidity

> **Question:** do reported pool activity, stablecoin state, or router impact
> describe a persistent liquidity-pressure context?

## Case map

- `crypto_defi_pool_flow_*`: DEX swap volume versus reported pool liquidity;
  response variants join the state with a BTC quote.
- `crypto_jupiter_route_impact_*`: configured quote-size ladder and router-
  reported impact; it is not a swap simulator.
- `crypto_meteora_dlmm_*`, `crypto_orca_whirlpool_*`, and
  `crypto_raydium_pool_concentration_*`: venue-specific pool snapshots,
  turnover, fee/TVL, dynamic fee, and concentration context.
- `crypto_stablecoin_depeg_*`, `crypto_stablecoin_rotation_response_replay.py`:
  pair deviation and later BTC-response diagnostics.
- `crypto_stablecoin_liquidity_*` and `crypto_stablecoin_liquidity_response_*`:
  DefiLlama supply snapshots and supply-change response research.
- `crypto_stablecoin_historical_response_replay.py`: bounded DefiLlama
  `peggedUSD` history aligned to MarketBridge BTC daily candles, so supply
  change can be tested without treating repeated current snapshots as history.
- `crypto_defi_funding_yield_risk_*` and `crypto_defi_yield_context_monitor.py`:
  yield/funding context with provider and coverage fields preserved.

## Recommended workflow

```bash
python3 examples/crypto/defi/crypto_defi_pool_flow_monitor.py \
  --symbol SOLUSDC --min-turnover-ratio 1.0
python3 examples/crypto/defi/crypto_defi_pool_flow_recorder.py \
  --symbol SOLUSDC --iterations 30 --interval-secs 60 \
  --output work/crypto-defi-pool-flow.jsonl
python3 examples/crypto/defi/crypto_defi_pool_flow_replay.py \
  --input work/crypto-defi-pool-flow.jsonl --min-run 3
```

For a BTC response study, use the matching `*_response_recorder.py` and
`*_response_replay.py` pair. The maintained command matrix and provider links
are in this guide.

Historical supply replay:

```bash
python3 examples/crypto/defi/crypto_stablecoin_historical_response_replay.py \
  --chain all --exchange binance --symbol BTCUSDT --interval 1d \
  --days 1825 --change-window-bars 7 --threshold-pct 1 \
  --horizon-bars 7 --min-observations 5
```

The replay consumes `/v1/history/stablecoins` and `/v1/history/candles`,
aligns by UTC calendar date, and preserves both endpoint coverage details.

## Evidence contract

Pool volume, TVL, fees, supply, route impact and stablecoin price are provider
fields—not proof of LP income, impermanent loss, solvency, causal flow, MEV,
gas, or executable depth. Pagination, stale snapshots, missing mints, and
provider model changes must remain visible. A partial or blocked upstream page
is `observe_only`, never a zero-liquidity claim.

## Boundary

The DeFi examples do not connect wallets, sign transactions, build swap routes,
estimate a guaranteed fill, or manage LP positions. They are read-only context
and replay tools.
