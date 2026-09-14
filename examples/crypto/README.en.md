# Crypto research families

MarketBridge keeps crypto strategy research in Python while Rust supplies the
connectors, normalization, history, caches and HTTP/WebSocket APIs. Each family
below is read-only and is organized by the evidence needed to test a falsifiable
hypothesis.

| Family | Primary evidence | Guide |
|---|---|---|
| Carry | basis, funding, cross-venue books, price gaps | [`carry/README.en.md`](carry/README.en.md) |
| DeFi | pool state, stablecoins, router impact, yield context | [`defi/README.en.md`](defi/README.en.md) |
| Macro | DXY/VIX/US10Y, ETF flow, aggregate regime | [`macro/README.en.md`](macro/README.en.md) |
| Microstructure | flow, OI, depth, liquidation, volatility | [`microstructure/README.en.md`](microstructure/README.en.md) |
| On-chain | transfers, mempool, mining | [`onchain/README.en.md`](onchain/README.en.md) |
| Options | skew, term structure, gamma, VRP, max pain | [`options/README.en.md`](options/README.en.md) |
| Sentiment | fear/greed, news, social metrics | [`sentiment/README.en.md`](sentiment/README.en.md) |
| Universe | breadth, rankings, pairs, regime context | [`universe/README.en.md`](universe/README.en.md) |

The shared [`strategy/README.en.md`](strategy/README.en.md) runner provides a
small Python CLI for common read-only cases. It is a convenience layer; the
family guides remain the source of truth for each hypothesis and limitation.

## File roles

- `*_monitor.py` reads a current MarketBridge response and prints structured evidence.
- `*_recorder.py` freezes repeated observations to append-only JSONL.
- `*_replay.py` reads that archive or bounded history and reports distributions.
- `*_response_recorder.py` / `*_response_replay.py` add a synchronized BTC
  quote to test later response rather than declaring a one-shot signal.

Run a monitor before a recorder. Check provider, timestamp, coverage and missing
fields before interpreting a replay. The concise cross-family map is in
[`../README.md`](../README.md); each family guide contains the maintained case
inventory. The Chinese family guides are named
`README.zh-CN.md` in each directory.

## Shared boundary

No crypto example places orders, signs wallets, transfers funds, manages
positions, or claims live PnL. Costs, borrow, funding, latency, slippage,
liquidity, queue position and execution are explicit research gaps unless a
case documents a paper assumption.
