# Source Expansion Inventory

This document tracks external connector coverage that MarketBridge may import
into its roadmap. It is intentionally an inventory, not an implementation claim.
Only sources listed as `implemented` in `docs/feature_inventory.md` are wired
into the runtime/API today.

Last checked: 2026-05-21

## Source References

- CCXT npm package `ccxt@4.5.54`: 110 exchange ids from `ccxt.exchanges`.
- CCXT wiki `Exchange-Markets`: 107 exchange markets shown on 2026-05-13.
- Hummingbot docs `CLOB Connectors`: current CLOB connector table.
- Hummingbot GitHub `hummingbot/connector/exchange`: 27 spot connector folders.
- Hummingbot GitHub `hummingbot/connector/derivative`: 18 perpetual connector folders.
- Hummingbot Gateway docs: 6 active DEX connectors and 6 legacy DEX connectors.

The CCXT package and wiki counts can differ because package exports include
aliases or transitional ids such as `gateio`, `huobi`, and `coinbaseadvanced`.

## Practical Strategy

MarketBridge should not blindly enable every listed venue. The data-source goal
is maximum useful coverage with freshness, schema consistency, and operational
visibility. A source enters the runtime only after it has at least:

- public REST/WS data contract documented;
- symbol normalization into MarketBridge canonical ids;
- health, stale, reconnect, and backpressure behavior;
- tests for parser edge cases;
- `feature_inventory.md` status update.

## Priority Buckets

| Priority | Scope | Rationale |
|---|---|---|
| P0 | Close remaining non-Polymarket semantic gaps: Vertex funding/OI, XRPL trades, keyed Architect/Decibel OI validation | These are the only exchange/CLOB rows still marked `planned` for non-Polymarket data. |
| P1 | Add native DeFi state beyond quotes: pool liquidity, route depth, swaps/trades for Jupiter/Raydium/Orca/Meteora/Uniswap/Curve/Balancer/SushiSwap/QuickSwap/Trader Joe/ETCSwap | Quote and price snapshots are wired; native state requires chain-specific APIs or indexers. |
| P2 | Complete options websocket parity for Deribit/OKX/Bybit/Binance | REST chains and per-instrument depth are wired; WS parity is a latency upgrade. |
| P3 | Add new long-tail centralized venues as native REST snapshot sources only when they add useful coverage | Broad coverage is useful for research, but schema quality and operational behavior come first. |
| P4 | Wallet/order/account-only capabilities | Out of scope unless MarketBridge grows an execution subsystem. |

## CCXT Exchange Ids

Current package list from `ccxt@4.5.54`:

```text
aftermath
alpaca
apex
arkham
ascendex
aster
backpack
bequant
bigone
binance
binancecoinm
binanceus
binanceusdm
bingx
bit2c
bitbank
bitbns
bitfinex
bitflyer
bitget
bithumb
bitmart
bitmex
bitopro
bitrue
bitso
bitstamp
bitteam
bittrade
bitvavo
blockchaincom
btcbox
btcmarkets
btcturk
bullish
bybit
bydfi
cex
coinbase
coinbaseinternational
coincheck
coinex
coinmate
coinmetro
coinsph
coinspot
cryptocom
cryptomus
deepcoin
delta
derive
digifinex
dydx
exmo
fmfwio
foxbit
gate
gateio
gemini
grvt
hashkey
hibachi
hitbtc
hollaex
htx
huobi
hyperliquid
independentreserve
indodax
kraken
krakenfutures
kucoin
kucoinfutures
latoken
lbank
lighter
luno
mercado
mexc
modetrade
myokx
ndax
novadax
okx
okxus
onetrading
oxfun
p2b
pacifica
paradex
paymium
phemex
poloniex
tokocrypto
toobit
upbit
wavesexchange
weex
whitebit
woo
woofipro
xt
yobit
zaif
zebpay
```

## Hummingbot CLOB Connectors

Spot connector folders in `hummingbot/connector/exchange`:

```text
ascend_ex
backpack
binance
bing_x
bitget
bitmart
bitrue
bitstamp
btc_markets
bybit
coinbase_advanced_trade
cube
derive
dexalot
foxbit
gate_io
htx
hyperliquid
injective_v2
kraken
kucoin
mexc
ndax
okx
paper_trade
vertex
xrpl
```

Perpetual connector folders in `hummingbot/connector/derivative`:

```text
aevo_perpetual
architect_perpetual
backpack_perpetual
binance_perpetual
bitget_perpetual
bitmart_perpetual
bybit_perpetual
decibel_perpetual
derive_perpetual
dydx_v4_perpetual
evedex_perpetual
gate_io_perpetual
grvt_perpetual
hyperliquid_perpetual
injective_v2_perpetual
kucoin_perpetual
okx_perpetual
pacifica_perpetual
```

Hummingbot Gateway active DEX connectors:

```text
jupiter
meteora
raydium
orca
uniswap
pancakeswap
```

Hummingbot Gateway legacy DEX connectors:

```text
balancer
curve
sushiswap
quickswap
traderjoe
etcswap
```

## Current MarketBridge Coverage Against These Lists

Already implemented or partially implemented in MarketBridge:

```text
aevo
architect
backpack
binance
bingx / bing_x
bitbank
bitfinex
bitget
bitmart
bitmex
blofin
bithumb
bitflyer
bitstamp
bitvavo
btc_markets
bullish
bybit
derive
dexalot
coinbase
coincheck
coinex
coinone
cryptocom
deribit
dydx
gate / gate_io
gemini
grvt
htx
hyperliquid
injective
kraken
kucoin
mexc
okx
pacifica
phemex
upbit
vertex
woo
xrpl
```

Already implemented non-CLOB or external sources:

```text
balancer
coingecko
coincap
coinmarketcap
coinglass
cryptopanic
curve
etherscan
etcswap
fear_greed
fred/us10y
jupiter
lunarcrush
mempool_space
meteora
oneinch
orca
pancakeswap
paraswap
polymarket
quickswap
raydium
santiment
sushiswap
traderjoe
uniswap_v3
whale_alert
yahoo/dxy
yahoo/vix
```

## Recommended Next Work

Most high-value CEX/perp sources in the earlier wave are now implemented or
explicitly marked unavailable for specific domains in
[`feature_inventory.md`](feature_inventory.md). The next practical work is:

1. Vertex funding/OI: wire only after stable public query behavior is verified.
2. XRPL trades: use ledger/indexer semantics; never synthesize trades from
   `book_offers` snapshots.
3. Architect/Decibel OI: validate with credentials before normalizing keyed OI.
4. DeFi native state: add chain-specific pool liquidity, route depth, and swap
   feeds where reliable public/indexed data exists.
5. Options WS parity: add low-latency WS ticker/book/trades on top of the
   existing REST chain/depth coverage.
6. Aggregator analytics: turn funding/OI/trade/liquidation events into explicit
   signal models.

For each new CEX source, implement in this order:

1. REST metadata and symbol normalization.
2. BBO/ticker.
3. L2 book.
4. Trades.
5. Funding and open interest for derivatives.
6. Liquidations when public and reliable.

This keeps MarketBridge useful after every connector, instead of waiting for a
large all-or-nothing integration.
