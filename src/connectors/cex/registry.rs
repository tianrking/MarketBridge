use std::sync::Arc;

mod symbols;

use crate::config::AppConfig;
use crate::connectors::aggregate::coincap::CoinCapPricePoller;
use crate::connectors::aggregate::coingecko::CoinGeckoPricePoller;
use crate::connectors::aggregate::coinglass::CoinGlassPoller;
use crate::connectors::aggregate::coinmarketcap::CoinMarketCapPricePoller;
use crate::connectors::aggregate::custom_api::CustomApiPoller;
use crate::connectors::defi::jupiter::JupiterQuotePoller;
use crate::connectors::defi::oneinch::OneInchQuotePoller;
use crate::connectors::defi::paraswap::ParaswapQuotePoller;
use crate::connectors::defi::raydium::RaydiumPricePoller;
use crate::connectors::defi::uniswap_v3::UniswapV3PoolPoller;
use crate::connectors::sentiment::cryptopanic::CryptoPanicPoller;
use crate::connectors::sentiment::fear_greed::FearGreedPoller;
use crate::connectors::sentiment::lunarcrush::LunarCrushPoller;
use crate::connectors::sentiment::santiment::SantimentPoller;
use crate::connectors::tradfi::fred::FredSeriesPoller;
use crate::connectors::tradfi::yahoo::YahooChartPoller;
use crate::source::ExchangeSource;

use super::aevo::AevoPerpFeed;
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
use super::bitmart::{BitmartPerpFeed, BitmartSpotFeed};
use super::bitstamp::BitstampSpotFeed;
use super::btc_markets::BtcMarketsSpotFeed;
use super::bybit::{BybitDepthFeed, BybitLiquidationFeed, BybitSpotTicker, BybitTradeFeed};
use super::bybit_perp::BybitPerpTicker;
use super::coinbase::CoinbaseTicker;
use super::derive::{DerivePerpFeed, DeriveSpotFeed};
use super::dexalot::DexalotSpotFeed;
use super::dydx::DydxFeed;
use super::gate::GateSpotBookTicker;
use super::gate_perp::GatePerpBookTicker;
use super::grvt::GrvtPerpFeed;
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
use super::pacifica::PacificaPerpFeed;
use super::vertex::{VertexFeed, VertexMarket};
use symbols::{
    split_quote, to_binance, to_bitfinex, to_bitfinex_perp, to_dash, to_dydx_market, to_htx_perp,
    to_hyperliquid_coin, to_kraken_perp, to_kucoin_perp, to_okx, to_okx_swap, to_slash,
    to_underscore,
};

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
            "hyperliquid" if !perp_symbols.is_empty() => {
                out.push(Arc::new(HyperliquidFeed::new(
                    perp_symbols
                        .iter()
                        .map(|s| to_hyperliquid_coin(s))
                        .collect(),
                )));
            }
            "dydx" if !perp_symbols.is_empty() => {
                out.push(Arc::new(DydxFeed::new(
                    perp_symbols.iter().map(|s| to_dydx_market(s)).collect(),
                )));
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
            "bingx" if !perp_symbols.is_empty() => {
                out.push(Arc::new(BingxSwapFeed::new(
                    perp_symbols.iter().map(|s| to_dash(s)).collect(),
                )));
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
            "bitmart" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(BitmartSpotFeed::new(
                        spot_symbols.iter().map(|s| to_underscore(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(BitmartPerpFeed::new(
                        perp_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
            }
            "bitstamp" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BitstampSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
                )));
            }
            "btc_markets" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BtcMarketsSpotFeed::new(
                    spot_symbols.iter().map(|s| to_dash(s)).collect(),
                )));
            }
            "aevo" if !perp_symbols.is_empty() => {
                out.push(Arc::new(AevoPerpFeed::new(
                    perp_symbols.iter().map(|s| to_aevo_perp(s)).collect(),
                )));
            }
            "pacifica" if !perp_symbols.is_empty() => {
                out.push(Arc::new(PacificaPerpFeed::new(
                    perp_symbols.iter().map(|s| to_pacifica_perp(s)).collect(),
                )));
            }
            "grvt" if !perp_symbols.is_empty() => {
                out.push(Arc::new(GrvtPerpFeed::new(
                    perp_symbols.iter().map(|s| to_grvt_perp(s)).collect(),
                )));
            }
            "vertex" => {
                let markets = vertex_markets(&spot_symbols, &perp_symbols);
                if !markets.is_empty() {
                    out.push(Arc::new(VertexFeed::new(markets)));
                }
            }
            "derive" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(DeriveSpotFeed::new(
                        spot_symbols.iter().map(|s| to_dash(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(DerivePerpFeed::new(
                        perp_symbols.iter().map(|s| to_derive_perp(s)).collect(),
                    )));
                }
            }
            "dexalot" if !spot_symbols.is_empty() => {
                out.push(Arc::new(DexalotSpotFeed::new(
                    spot_symbols.iter().map(|s| to_slash(s)).collect(),
                )));
            }
            "coinbase" if !spot_symbols.is_empty() => {
                out.push(Arc::new(CoinbaseTicker::new(
                    spot_symbols.iter().map(|s| to_dash(s)).collect(),
                )));
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

    if cfg.defi.jupiter.enabled {
        out.push(Arc::new(JupiterQuotePoller::new(cfg.defi.jupiter.clone())));
    }
    if cfg.defi.raydium.enabled {
        out.push(Arc::new(RaydiumPricePoller::new(cfg.defi.raydium.clone())));
    }
    if cfg.defi.uniswap_v3.enabled {
        out.push(Arc::new(UniswapV3PoolPoller::new(
            cfg.defi.uniswap_v3.clone(),
        )));
    }
    if cfg.defi.paraswap.enabled {
        out.push(Arc::new(ParaswapQuotePoller::new(
            cfg.defi.paraswap.clone(),
        )));
    }
    if cfg.defi.oneinch.enabled {
        out.push(Arc::new(OneInchQuotePoller::new(cfg.defi.oneinch.clone())));
    }
    if cfg.tradfi.dxy.enabled {
        out.push(Arc::new(YahooChartPoller::new(
            "dxy",
            cfg.tradfi.dxy.clone(),
        )));
    }
    if cfg.tradfi.vix.enabled {
        out.push(Arc::new(YahooChartPoller::new(
            "vix",
            cfg.tradfi.vix.clone(),
        )));
    }
    if cfg.tradfi.us10y.enabled {
        out.push(Arc::new(FredSeriesPoller::new(
            "us10y",
            cfg.tradfi.us10y.clone(),
        )));
    }
    if cfg.aggregates.coingecko.enabled {
        out.push(Arc::new(CoinGeckoPricePoller::new(
            cfg.aggregates.coingecko.clone(),
        )));
    }
    if cfg.aggregates.coincap.enabled {
        out.push(Arc::new(CoinCapPricePoller::new(
            cfg.aggregates.coincap.clone(),
        )));
    }
    if cfg.aggregates.coinmarketcap.enabled {
        out.push(Arc::new(CoinMarketCapPricePoller::new(
            cfg.aggregates.coinmarketcap.clone(),
        )));
    }
    if cfg.aggregates.coinglass.enabled {
        out.push(Arc::new(CoinGlassPoller::new(
            cfg.aggregates.coinglass.clone(),
        )));
    }
    for custom_api in cfg.aggregates.custom_apis.iter().filter(|api| api.enabled) {
        out.push(Arc::new(CustomApiPoller::new(custom_api.clone())));
    }
    if cfg.sentiment.fear_greed.enabled {
        out.push(Arc::new(FearGreedPoller::new(
            cfg.sentiment.fear_greed.clone(),
        )));
    }
    if cfg.sentiment.cryptopanic.enabled {
        out.push(Arc::new(CryptoPanicPoller::new(
            cfg.sentiment.cryptopanic.clone(),
        )));
    }
    if cfg.sentiment.santiment.enabled {
        out.push(Arc::new(SantimentPoller::new(
            cfg.sentiment.santiment.clone(),
        )));
    }
    if cfg.sentiment.lunarcrush.enabled {
        out.push(Arc::new(LunarCrushPoller::new(
            cfg.sentiment.lunarcrush.clone(),
        )));
    }

    out
}

fn to_derive_perp(symbol: &str) -> String {
    if symbol.contains('-') {
        return symbol.to_ascii_uppercase();
    }
    if let Some(base) = symbol.strip_suffix("PERP") {
        return format!("{}-PERP", base.to_ascii_uppercase());
    }
    let (base, _) = split_quote(symbol);
    format!("{}-PERP", base.to_ascii_uppercase())
}

fn to_aevo_perp(symbol: &str) -> String {
    if symbol.contains('-') {
        return symbol.to_ascii_uppercase();
    }
    if let Some(base) = symbol.strip_suffix("PERP") {
        return format!("{}-PERP", base.to_ascii_uppercase());
    }
    let (base, _) = split_quote(symbol);
    format!("{}-PERP", base.to_ascii_uppercase())
}

fn to_pacifica_perp(symbol: &str) -> String {
    if let Some((base, _)) = symbol.split_once('-') {
        return base.to_ascii_uppercase();
    }
    let upper = symbol.to_ascii_uppercase();
    if let Some(base) = upper.strip_suffix("PERP") {
        return base.to_string();
    }
    split_quote(&upper).0.to_string()
}

fn to_grvt_perp(symbol: &str) -> String {
    if symbol.contains("_") {
        return symbol.to_string();
    }
    if let Some(base) = symbol.strip_suffix("-PERP") {
        let compact = base.replace('-', "");
        let (base, quote) = split_quote(&compact);
        return format!("{}_{}_Perp", base, quote);
    }
    let upper = symbol.to_ascii_uppercase();
    let (base, quote) = split_quote(&upper);
    format!("{}_{}_Perp", base, quote)
}

fn vertex_markets(spot_symbols: &[String], perp_symbols: &[String]) -> Vec<VertexMarket> {
    let mut markets = Vec::new();
    for symbol in spot_symbols {
        if let Some((product_id, symbol)) = vertex_spot_market(symbol) {
            markets.push(VertexMarket::new(
                product_id,
                symbol,
                crate::types::MarketKind::Spot,
            ));
        }
    }
    for symbol in perp_symbols {
        if let Some((product_id, symbol)) = vertex_perp_market(symbol) {
            markets.push(VertexMarket::new(
                product_id,
                symbol,
                crate::types::MarketKind::Perp,
            ));
        }
    }
    markets
}

fn vertex_spot_market(symbol: &str) -> Option<(u64, &'static str)> {
    match compact_symbol(symbol).as_str() {
        "WBTCUSDC" | "BTCUSDC" => Some((1, "wBTC/USDC")),
        "WETHUSDC" | "ETHUSDC" => Some((3, "wETH/USDC")),
        "ARBUSDC" => Some((5, "ARB/USDC")),
        _ => None,
    }
}

fn vertex_perp_market(symbol: &str) -> Option<(u64, &'static str)> {
    match compact_symbol(symbol).as_str() {
        "BTCPERP" | "BTCUSDC" | "BTCUSDT" => Some((2, "BTC-PERP")),
        "ETHPERP" | "ETHUSDC" | "ETHUSDT" => Some((4, "ETH-PERP")),
        "ARBPERP" | "ARBUSDC" | "ARBUSDT" => Some((6, "ARB-PERP")),
        _ => None,
    }
}

fn compact_symbol(symbol: &str) -> String {
    symbol
        .chars()
        .filter(|ch| !matches!(ch, '-' | '/' | '_'))
        .collect::<String>()
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AggregatesConfig, AppConfig, BinanceOptionsConfig, BybitOptionsConfig, DefiConfig,
        DeribitConfig, ExchangeConfig, KlineConfig, OkxOptionsConfig, OnchainConfig,
        PolymarketConfig, RuntimeConfig, SentimentConfig, StrategyConfig, TradfiConfig,
        fees::FeeModel, runtime::BackpressureConfig,
    };
    use std::collections::HashMap;

    #[test]
    fn build_sources_creates_enabled_spot_and_perp_adapters() {
        let cfg = AppConfig {
            runtime: RuntimeConfig {
                queue_capacity: 16,
                broadcast_capacity: 16,
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
                fee_mode: crate::config::StrategyFeeMode::Taker,
            },
            deribit: DeribitConfig::default(),
            okx_options: OkxOptionsConfig::default(),
            bybit_options: BybitOptionsConfig::default(),
            binance_options: BinanceOptionsConfig::default(),
            polymarket: PolymarketConfig::default(),
            defi: DefiConfig::default(),
            tradfi: TradfiConfig::default(),
            aggregates: AggregatesConfig::default(),
            sentiment: SentimentConfig::default(),
            klines: KlineConfig::default(),
            onchain: OnchainConfig::default(),
            symbols: vec!["BTCUSDT".to_string()],
            perp_symbols: Some(vec!["BTCUSDT".to_string()]),
            exchanges: HashMap::from([(
                "binance".to_string(),
                ExchangeConfig {
                    enabled: true,
                    symbols: None,
                    perp_symbols: None,
                    fee: FeeModel::Fixed {
                        maker_bps: 1.0,
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

    #[test]
    fn aevo_perp_symbol_converter_uses_instrument_name() {
        assert_eq!(to_aevo_perp("ETHUSDT"), "ETH-PERP");
        assert_eq!(to_aevo_perp("BTC-PERP"), "BTC-PERP");
        assert_eq!(to_aevo_perp("SOLPERP"), "SOL-PERP");
    }

    #[test]
    fn pacifica_perp_symbol_converter_uses_base_symbol() {
        assert_eq!(to_pacifica_perp("BTCUSDT"), "BTC");
        assert_eq!(to_pacifica_perp("ETH-PERP"), "ETH");
        assert_eq!(to_pacifica_perp("SOLPERP"), "SOL");
    }

    #[test]
    fn grvt_perp_symbol_converter_uses_instrument_name() {
        assert_eq!(to_grvt_perp("BTCUSDT"), "BTC_USDT_Perp");
        assert_eq!(to_grvt_perp("ETHUSDT"), "ETH_USDT_Perp");
        assert_eq!(to_grvt_perp("SOL_USDT_Perp"), "SOL_USDT_Perp");
    }

    #[test]
    fn vertex_markets_map_known_product_ids() {
        let spot = vec!["WBTCUSDC".to_string(), "ETHUSDC".to_string()];
        let perp = vec!["BTCUSDT".to_string(), "ETH-PERP".to_string()];
        let markets = vertex_markets(&spot, &perp);
        let ids = markets
            .iter()
            .map(|market| market.product_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![1, 3, 2, 4]);
    }
}
