# Source Expansion Inventory

This document tracks external connector coverage that MarketBridge may import
into its roadmap. It is intentionally an inventory, not an implementation claim.
Only sources listed as `implemented` in `docs/feature_inventory.md` are wired
into the runtime/API today.

Last checked: 2026-05-20

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
| P0 | Finish depth on existing high-liquidity sources: Binance, OKX, Bybit, Bitget, KuCoin, Gate, Kraken, HTX, Bitfinex, Coinbase, MEXC, BingX, Backpack, Hyperliquid, dYdX | Highest immediate value; reduces partial rows before adding long tail. |
| P1 | Add Hummingbot overlap not yet implemented: Injective, XRPL, Architect, Decibel | Hummingbot already validates these as trading connectors, useful for implementation patterns and liquidity discovery. |
| P2 | Add liquid CCXT/CEX venues: BitMEX, Crypto.com, CoinEx, Gemini, HashKey, Bitvavo, Bullish, WOO X, Phemex, Poloniex, Upbit, Bithumb | Strong market-data utility; good candidates for REST-first then WS. |
| P3 | Add CCXT long-tail venues as REST snapshot sources | Broad coverage for research, but lower operational priority. |
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
blofin
btcbox
btcmarkets
btcturk
bullish
bybit
bydfi
cex
coinbase
coinbaseadvanced
coinbaseexchange
coinbaseinternational
coincheck
coinex
coinmate
coinmetro
coinone
coinsph
coinspot
cryptocom
cryptomus
deepcoin
delta
deribit
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
backpack
binance
bingx / bing_x
bitfinex
bitget
bitmart
bitstamp
btc_markets
bybit
derive
dexalot
coinbase
deribit
dydx
gate / gate_io
grvt
htx
hyperliquid
kraken
kucoin
mexc
okx
pacifica
vertex
```

Already implemented non-CLOB or external sources:

```text
coingecko
coincap
coinmarketcap
coinglass
cryptopanic
etherscan
fear_greed
fred/us10y
jupiter
lunarcrush
mempool_space
oneinch
paraswap
polymarket
raydium
santiment
uniswap_v3
whale_alert
yahoo/dxy
yahoo/vix
```

## Recommended Next Connectors

The next practical implementation wave should be:

1. Crypto.com spot/perp: CCXT certified, high retail flow.
2. BitMEX perp: important derivatives reference, strong public market data.
3. Gemini spot: regulated US reference venue.
4. CoinEx spot/perp: CCXT certified, useful long-tail liquidity.
5. Phemex spot/perp: derivatives venue with clear public feeds.
6. Upbit/Bithumb spot: KRW market regime signal.
7. Injective/Vertex/Dexalot: CLOB DEX sources from Hummingbot coverage.
8. Meteora/Orca/PancakeSwap/Balancer/Curve/SushiSwap: DeFi depth and pool-state expansion.

For each new CEX source, implement in this order:

1. REST metadata and symbol normalization.
2. BBO/ticker.
3. L2 book.
4. Trades.
5. Funding and open interest for derivatives.
6. Liquidations when public and reliable.

This keeps MarketBridge useful after every connector, instead of waiting for a
large all-or-nothing integration.
