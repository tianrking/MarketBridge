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
  spot-only exchanges with no funding rate.

Recent parity closes:

| Source | Closed gap | Normalized event |
|---|---|---|
| Hyperliquid | `activeAssetCtx` no longer drops OI when funding is present. | `FundingRate`, `OpenInterest` |
| Bitrue | Public recent spot trades added from the Bitrue REST trade endpoint. | `Trade` |
| Foxbit | Public recent spot trades added from the Foxbit market trade-history endpoint. | `Trade` |
| NDAX | Public recent spot trades added from `GetLastTrades` after instrument resolution. | `Trade` |
| Gate | Public USDT perpetual REST book, trades, contract funding/OI, and liquidation orders added beside the existing book-ticker stream. | `OrderBook`, `Trade`, `FundingRate`, `OpenInterest`, `Liquidation` |
| HTX | Public linear-swap REST book, trades, funding, and open interest added beside the existing BBO stream. | `OrderBook`, `Trade`, `FundingRate`, `OpenInterest` |

Remaining high-value CCXT parity queue:

| Priority | Source group | Gap |
|---|---|---|
| P0 | Bitfinex perps | Replace perp BBO-only paths with full book/trade/funding/OI where stable public APIs exist. |
| P1 | MEXC / BingX / Bitmart | Confirm and wire public OI/liquidation equivalents where the venue exposes them without credentials. |
| P1 | Cube / XRPL | Add trade streams only after stable public semantics are confirmed; do not synthesize trades from book snapshots. |
| P2 | Backpack / Aevo / BloFin / Derive / Evedex | Add OI/liquidation only when a public endpoint is stable enough for production polling. |
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
