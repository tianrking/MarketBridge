use std::sync::Arc;

use crate::config::AppConfig;
use crate::source::ExchangeSource;

use super::backpack::BackpackFeed;
use super::binance::{
    BinanceBookTicker, BinanceDepthFeed, BinanceFundingTicker, BinanceLiquidationFeed,
    BinanceOpenInterestPoller, BinanceTradeFeed,
};
use super::binance_perp::BinancePerpBookTicker;
use super::bingx::BingxSwapFeed;
use super::bitfinex::BitfinexTicker;
use super::bitfinex_perp::BitfinexPerpTicker;
use super::bitget::BitgetSpotTicker;
use super::bitget_perp::BitgetPerpTicker;
use super::bybit::{BybitDepthFeed, BybitLiquidationFeed, BybitSpotTicker, BybitTradeFeed};
use super::bybit_perp::BybitPerpTicker;
use super::coinbase::CoinbaseTicker;
use super::dydx::DydxFeed;
use super::gate::GateSpotBookTicker;
use super::gate_perp::GatePerpBookTicker;
use super::htx::HtxBbo;
use super::htx_perp::HtxPerpBbo;
use super::hyperliquid::HyperliquidFeed;
use super::kraken::KrakenTicker;
use super::kraken_perp::KrakenPerpTicker;
use super::kucoin::KucoinTicker;
use super::kucoin_perp::KucoinPerpTicker;
use super::mexc::MexcFeed;
use super::okx::{
    OkxDepthFeed, OkxFundingFeed, OkxLiquidationPoller, OkxOpenInterestFeed, OkxTicker,
    OkxTradeFeed,
};
use super::okx_perp::OkxPerpTicker;

pub fn build_sources(cfg: &AppConfig) -> Vec<Arc<dyn ExchangeSource>> {
    let mut out: Vec<Arc<dyn ExchangeSource>> = Vec::new();

    for ex in cfg.enabled_exchanges() {
        let spot_symbols = cfg.symbols_for_exchange(&ex);
        let perp_symbols = cfg.perp_symbols_for_exchange(&ex);

        match ex.as_str() {
            "okx" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(OkxTicker::new(
                        spot_symbols.iter().map(|s| to_okx(s)).collect(),
                    )));
                    out.push(Arc::new(OkxDepthFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_okx(s)).collect(),
                    )));
                    out.push(Arc::new(OkxTradeFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_okx(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(OkxPerpTicker::new(
                        perp_symbols.iter().map(|s| to_okx_swap(s)).collect(),
                    )));
                    out.push(Arc::new(OkxFundingFeed::new(
                        perp_symbols.iter().map(|s| to_okx_swap(s)).collect(),
                    )));
                    out.push(Arc::new(OkxOpenInterestFeed::new(
                        perp_symbols.iter().map(|s| to_okx_swap(s)).collect(),
                    )));
                    out.push(Arc::new(OkxLiquidationPoller::new(
                        perp_symbols.iter().map(|s| to_okx_swap(s)).collect(),
                    )));
                    out.push(Arc::new(OkxDepthFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_okx_swap(s)).collect(),
                    )));
                    out.push(Arc::new(OkxTradeFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_okx_swap(s)).collect(),
                    )));
                }
            }
            "hyperliquid" => {
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(HyperliquidFeed::new(
                        perp_symbols
                            .iter()
                            .map(|s| to_hyperliquid_coin(s))
                            .collect(),
                    )));
                }
            }
            "dydx" => {
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(DydxFeed::new(
                        perp_symbols.iter().map(|s| to_dydx_market(s)).collect(),
                    )));
                }
            }
            "backpack" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(BackpackFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_underscore(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(BackpackFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_underscore(s)).collect(),
                    )));
                }
            }
            "mexc" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(MexcFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(MexcFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_underscore(s)).collect(),
                    )));
                }
            }
            "bingx" => {
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(BingxSwapFeed::new(
                        perp_symbols.iter().map(|s| to_dash(s)).collect(),
                    )));
                }
            }
            "bybit" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(BybitSpotTicker::new(
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BybitDepthFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BybitTradeFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(BybitPerpTicker::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BybitLiquidationFeed::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BybitDepthFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BybitTradeFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
            }
            "bitget" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(BitgetSpotTicker::new(
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(BitgetPerpTicker::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
            }
            "coinbase" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(CoinbaseTicker::new(
                        spot_symbols.iter().map(|s| to_dash(s)).collect(),
                    )));
                }
            }
            "kraken" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(KrakenTicker::new(
                        spot_symbols.iter().map(|s| to_slash(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(KrakenPerpTicker::new(
                        perp_symbols.iter().map(|s| to_kraken_perp(s)).collect(),
                    )));
                }
            }
            "kucoin" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(KucoinTicker::new(
                        spot_symbols.iter().map(|s| to_dash(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(KucoinPerpTicker::new(
                        perp_symbols.iter().map(|s| to_kucoin_perp(s)).collect(),
                    )));
                }
            }
            "gate" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(GateSpotBookTicker::new(
                        spot_symbols.iter().map(|s| to_underscore(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(GatePerpBookTicker::new(
                        perp_symbols.iter().map(|s| to_underscore(s)).collect(),
                    )));
                }
            }
            "binance" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(BinanceBookTicker::new(
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BinanceDepthFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BinanceTradeFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(BinancePerpBookTicker::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BinanceFundingTicker::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BinanceOpenInterestPoller::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BinanceLiquidationFeed::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BinanceDepthFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                    out.push(Arc::new(BinanceTradeFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
            }
            "htx" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(HtxBbo::new(
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(HtxPerpBbo::new(
                        perp_symbols.iter().map(|s| to_htx_perp(s)).collect(),
                    )));
                }
            }
            "bitfinex" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(BitfinexTicker::new(
                        spot_symbols.iter().map(|s| to_bitfinex(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(BitfinexPerpTicker::new(
                        perp_symbols.iter().map(|s| to_bitfinex_perp(s)).collect(),
                    )));
                }
            }
            _ => {}
        }
    }

    out
}

fn split_quote(s: &str) -> (&str, &str) {
    for q in ["USDT", "USDC", "USD", "BTC", "ETH"] {
        if let Some(base) = s.strip_suffix(q) {
            return (base, q);
        }
    }
    if s.len() >= 6 {
        let (b, q) = s.split_at(s.len() - 4);
        return (b, q);
    }
    (s, "USDT")
}

fn to_binance(s: &str) -> String {
    s.to_string()
}
fn to_okx(s: &str) -> String {
    to_dash(s)
}
fn to_okx_swap(s: &str) -> String {
    format!("{}-SWAP", to_dash(s))
}
fn to_dash(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("{}-{}", b, q)
}
fn to_slash(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("{}/{}", b, q)
}
fn to_underscore(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("{}_{}", b, q)
}
fn to_bitfinex(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("t{}{}", b, q)
}
fn to_kucoin_perp(s: &str) -> String {
    format!("{}M", s)
}
fn to_htx_perp(s: &str) -> String {
    to_dash(s)
}
fn to_bitfinex_perp(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("t{}F0:{}F0", b, q)
}
fn to_kraken_perp(s: &str) -> String {
    // Kraken futures symbols vary across venues; pass through for user override compatibility.
    s.to_string()
}
fn to_hyperliquid_coin(s: &str) -> String {
    split_quote(s).0.to_string()
}
fn to_dydx_market(s: &str) -> String {
    let (base, quote) = split_quote(s);
    format!("{base}-{quote}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AppConfig, BackpressureConfig, DeribitConfig, ExchangeConfig, FeeModel, PolymarketConfig,
        RuntimeConfig, StrategyConfig,
    };
    use std::collections::HashMap;

    #[test]
    fn symbol_converters_work_for_usdt_pairs() {
        assert_eq!(to_okx("BTCUSDT"), "BTC-USDT");
        assert_eq!(to_okx_swap("ETHUSDT"), "ETH-USDT-SWAP");
        assert_eq!(to_underscore("BTCUSDT"), "BTC_USDT");
        assert_eq!(to_slash("ETHUSDT"), "ETH/USDT");
        assert_eq!(to_bitfinex("BTCUSDT"), "tBTCUSDT");
        assert_eq!(to_kucoin_perp("BTCUSDT"), "BTCUSDTM");
        assert_eq!(to_htx_perp("BTCUSDT"), "BTC-USDT");
        assert_eq!(to_bitfinex_perp("BTCUSDT"), "tBTCF0:USDTF0");
        assert_eq!(to_hyperliquid_coin("BTCUSDT"), "BTC");
        assert_eq!(to_dydx_market("BTCUSDT"), "BTC-USDT");
    }

    #[test]
    fn build_sources_creates_enabled_spot_and_perp_adapters() {
        let cfg = AppConfig {
            runtime: RuntimeConfig {
                queue_capacity: 16,
                backpressure: BackpressureConfig::Block,
                report_interval_ms: 1000,
                stale_ttl_ms: 1000,
                api_addr: "127.0.0.1:0".to_string(),
                redis_url: None,
                redis_stream_prefix: "ticks".to_string(),
            },
            strategy: StrategyConfig {
                min_profit_usdt: 1.0,
                min_profit_bps: 1.0,
                min_signal_hold_ms: 0,
                slippage_bps: 0.0,
            },
            deribit: DeribitConfig::default(),
            polymarket: PolymarketConfig::default(),
            symbols: vec!["BTCUSDT".to_string()],
            perp_symbols: Some(vec!["BTCUSDT".to_string()]),
            exchanges: HashMap::from([(
                "binance".to_string(),
                ExchangeConfig {
                    enabled: true,
                    symbols: None,
                    perp_symbols: None,
                    fee: FeeModel::Fixed {
                        _maker_bps: 1.0,
                        taker_bps: 2.0,
                    },
                },
            )]),
        };

        let source_names = build_sources(&cfg)
            .into_iter()
            .map(|source| source.name())
            .collect::<Vec<_>>();
        assert_eq!(
            source_names,
            vec![
                "binance", "binance", "binance", "binance", "binance", "binance", "binance",
                "binance", "binance"
            ]
        );
    }
}
