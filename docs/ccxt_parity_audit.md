# CCXT Parity Audit

MarketBridge is not a CCXT wrapper. It keeps a standalone Rust runtime and uses
the local CCXT tree as a reference checklist for public market-data coverage.
That keeps binary releases self-contained while still pushing connector breadth
toward CCXT-style completeness.

Audit source:

- CCXT reference tree: `/home/w0x7ce/Downloads/OKTRADER/ccxt/js/src`
- MarketBridge native connectors: `src/connectors/cex`
- Runtime status endpoint: `GET /v1/catalog/source-roadmap`
- Feature matrix: `docs/feature_inventory.md`

Current shape:

- The local CCXT JavaScript tree contains 110 exchange implementation files.
- MarketBridge has 61 native CEX connector modules, plus DeFi, option,
  prediction-market, on-chain, sentiment, and TradFi sources.
- The full user-requested exchange set has native MarketBridge modules:
  Hyperliquid, dYdX, Backpack, MEXC, BingX, Bitget, Bitmart, BitMEX, Deribit,
  Phemex, CoinEx, Crypto.com, WOO X, BloFin, Aevo, Pacifica, GRVT, Injective,
  Derive, Evedex, Gate, HTX, Bitfinex, Bitstamp, Bitrue, AscendEX, BTC Markets,
  Dexalot, Vertex, XRPL, Cube, Foxbit, and NDAX.

Important boundary:

- `implemented` means MarketBridge emits normalized events for the listed
  domain from a public source.
- `partial` means the venue exists but some product type or event domain is not
  available, not stable, or not yet wired.
- `planned` means no normalized event is emitted for that domain yet.
- `n/a` means the venue/product does not naturally provide that domain, such as
  spot-only exchanges with no funding rate, or no stable public endpoint is
  known. `n/a` rows are not implementation gaps until a stable public source is
  confirmed.

Recent parity closes:

| Source | Closed gap | Normalized event |
|---|---|---|
| Hyperliquid | `activeAssetCtx` no longer drops OI when funding is present. | `FundingRate`, `OpenInterest` |
| Bitrue | Public recent spot trades added from the Bitrue REST trade endpoint. | `Trade` |
| Foxbit | Public recent spot trades added from the Foxbit market trade-history endpoint. | `Trade` |
| NDAX | Public recent spot trades added from `GetLastTrades` after instrument resolution. | `Trade` |
| Gate | Public USDT perpetual REST book, trades, contract funding/OI, and liquidation orders added beside the existing book-ticker stream. | `OrderBook`, `Trade`, `FundingRate`, `OpenInterest`, `Liquidation` |
| HTX | Public linear-swap REST book, trades, funding, open interest, and liquidation orders added beside the existing BBO stream. | `OrderBook`, `Trade`, `FundingRate`, `OpenInterest`, `Liquidation` |
| Bitfinex | Public derivatives REST book, trades, and liquidation history added beside the existing perp ticker stream. | `OrderBook`, `Trade`, `Liquidation` |
| Bitfinex | Public derivatives status endpoint added and the USDT perp symbol converter fixed to Bitfinex `USTF0` ids. | `FundingRate`, `OpenInterest` |
| BingX | Public swap premium-index funding and open-interest endpoints added beside the existing depth/trade stream; force-order liquidation history is private-only in CCXT. | `FundingRate`, `OpenInterest` |
| MEXC | Public contract funding-rate endpoint added beside the existing spot/futures depth and deals streams; CCXT marks public OI/liquidation fetches unavailable. | `FundingRate` |
| Bitmart | Public futures open-interest endpoint added beside existing depth/trade/funding streams; public liquidation fetch remains unavailable in CCXT. | `OpenInterest` |
| Cube | Public market-data REST recent trades added beside existing MBP snapshots with trade-id dedupe. | `Trade` |
| KuCoin | Futures REST depth, recent trades, funding, and open-interest metrics added; BTC futures id mapping fixed to `XBTUSDTM`. | `OrderBook`, `Trade`, `FundingRate`, `OpenInterest` |
| Kraken | Futures REST ticker, depth, recent trades, funding, and open-interest metrics added with `PF_` symbol mapping. | `OrderBook`, `Trade`, `FundingRate`, `OpenInterest` |
| BloFin | Public perp open-interest endpoint added beside existing REST ticker, depth, trades, and funding. | `OpenInterest` |
| Aevo | Public instrument `markets.total_oi` is emitted beside the existing REST funding and instrument quote path. | `OpenInterest` |
| Derive | Public ticker `open_interest` is emitted beside existing WS book/trade and perp funding paths. | `OpenInterest` |
| Evedex | Public instrument `openInterest` is emitted beside existing REST depth, trades, and funding metrics. | `OpenInterest` |
| Backpack | Public mark-price funding and open-interest endpoints are emitted beside existing WS book/trade paths. | `FundingRate`, `OpenInterest` |
| Injective | Public Sentry open-interest endpoint is emitted beside existing LCD/Sentry book, trade, and funding paths. | `OpenInterest` |
| CoinEx | Public futures liquidation history is emitted beside existing ticker, book, trade, funding, and OI paths. | `Liquidation` |

Remaining high-value CCXT parity queue:

| Priority | Source group | Gap |
|---|---|---|
| P1 | Vertex | Add funding/OI only after stable public query behavior is confirmed. |
| P1 | XRPL | Add trade streams only after stable public semantics are confirmed; do not synthesize trades from book snapshots. |
| P1 | Architect / Decibel | Validate keyed OI before normalizing it. |
| P2 | Perp liquidation long tail | Most venues without stable public liquidation feeds are now explicit `n/a`; add new feeds only when a reliable endpoint is confirmed. |
| P2 | Extra CCXT long tail | Add native Rust connectors by liquidity and strategy value, not by blindly wrapping every CCXT file. |

Rule for future work:

Every connector change must update both `docs/feature_inventory.md` and
`src/source_roadmap.rs`, then pass:

```bash
cargo fmt
cargo check
cargo test -- --nocapture
cargo clippy --all-targets --all-features -- -D warnings
```
