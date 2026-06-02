use std::sync::Arc;

mod aggregate;
mod binance;
mod bybit;
mod defi;
mod okx;
mod sentiment;
mod symbols;
mod tradfi;

use crate::config::AppConfig;
use crate::source::ExchangeSource;

use super::aevo::AevoPerpFeed;
use super::architect::ArchitectPerpFeed;
use super::ascend_ex::AscendExSpotFeed;
use super::backpack::BackpackFeed;
use super::bingx::{BingxSwapFeed, BingxSwapMetricsPoller};
use super::bitbank::BitbankSpotFeed;
use super::bitfinex::{BitfinexSpotRestFeed, BitfinexTicker};
use super::bitfinex_perp::{BitfinexPerpRestFeed, BitfinexPerpTicker};
use super::bitflyer::BitflyerSpotFeed;
use super::bitget::BitgetSpotTicker;
use super::bitget_perp::BitgetPerpTicker;
use super::bithumb::BithumbSpotFeed;
use super::bitmart::{BitmartPerpFeed, BitmartPerpMetricsPoller, BitmartSpotFeed};
use super::bitmex::BitmexPerpFeed;
use super::bitrue::BitrueSpotFeed;
use super::bitstamp::BitstampSpotFeed;
use super::bitvavo::BitvavoSpotFeed;
use super::blofin::BlofinPerpFeed;
use super::btc_markets::BtcMarketsSpotFeed;
use super::bullish::BullishSpotFeed;
use super::coinbase::{CoinbaseRestFeed, CoinbaseTicker};
use super::coincheck::CoincheckSpotFeed;
use super::coinex::CoinexFeed;
use super::coinone::CoinoneSpotFeed;
use super::cryptocom::CryptoComFeed;
use super::cube::CubeSpotFeed;
use super::decibel::DecibelPerpFeed;
use super::deribit::DeribitPerpFeed;
use super::derive::{DerivePerpFeed, DeriveSpotFeed};
use super::dexalot::DexalotSpotFeed;
use super::dydx::DydxFeed;
use super::evedex::EvedexPerpFeed;
use super::foxbit::FoxbitSpotFeed;
use super::gate::{GateSpotBookTicker, GateSpotRestFeed};
use super::gate_perp::{GatePerpBookTicker, GatePerpRestFeed};
use super::gemini::GeminiSpotFeed;
use super::grvt::GrvtPerpFeed;
use super::htx::{HtxBbo, HtxSpotRestFeed};
use super::htx_perp::{HtxPerpBbo, HtxPerpRestFeed};
use super::hyperliquid::HyperliquidFeed;
use super::injective::InjectiveFeed;
use super::kraken::{KrakenRestFeed, KrakenTicker};
use super::kraken_perp::{KrakenPerpRestFeed, KrakenPerpTicker};
use super::kucoin::KucoinTicker;
use super::kucoin_perp::{KucoinPerpRestFeed, KucoinPerpTicker};
use super::kucoin_rest::KucoinRestFeed;
use super::mexc::{MexcFeed, MexcFundingPoller};
use super::ndax::NdaxSpotFeed;
use super::pacifica::PacificaPerpFeed;
use super::phemex::PhemexPerpFeed;
use super::upbit::UpbitSpotFeed;
use super::vertex::{VertexFeed, VertexMarket};
use super::woo::WooFeed;
use super::xrpl::{XrplPair, XrplSpotFeed};
use symbols::{
    split_quote, to_binance, to_bitfinex, to_bitfinex_perp, to_dash, to_dydx_market, to_htx_perp,
    to_hyperliquid_coin, to_kraken_perp, to_kucoin_perp, to_slash, to_underscore,
};

pub(super) struct RegistryContext<'a> {
    pub cfg: &'a AppConfig,
    pub spot_symbols: &'a [String],
    pub perp_symbols: &'a [String],
}

pub fn build_sources(cfg: &AppConfig) -> Vec<Arc<dyn ExchangeSource>> {
    let mut out: Vec<Arc<dyn ExchangeSource>> = Vec::new();

    for ex in cfg.enabled_exchanges() {
        let spot_symbols = cfg.symbols_for_exchange(&ex);
        let perp_symbols = cfg.perp_symbols_for_exchange(&ex);
        let ctx = RegistryContext {
            cfg,
            spot_symbols: &spot_symbols,
            perp_symbols: &perp_symbols,
        };

        match ex.as_str() {
            "okx" => okx::push_sources(&mut out, &ctx),
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
                    let mexc_perp_symbols = perp_symbols
                        .iter()
                        .map(|s| to_underscore(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(MexcFeed::new(
                        crate::types::MarketKind::Perp,
                        mexc_perp_symbols.clone(),
                    )));
                    out.push(Arc::new(MexcFundingPoller::new(mexc_perp_symbols)));
                }
            }
            "bingx" if !perp_symbols.is_empty() => {
                let bingx_symbols = perp_symbols.iter().map(|s| to_dash(s)).collect::<Vec<_>>();
                out.push(Arc::new(BingxSwapFeed::new(bingx_symbols.clone())));
                out.push(Arc::new(BingxSwapMetricsPoller::new(bingx_symbols)));
            }
            "blofin" if !perp_symbols.is_empty() => {
                out.push(Arc::new(BlofinPerpFeed::new(
                    perp_symbols.iter().map(|s| to_dash(s)).collect(),
                )));
            }
            "bitbank" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BitbankSpotFeed::new(
                    spot_symbols
                        .iter()
                        .map(|s| to_underscore(s).to_ascii_lowercase())
                        .collect(),
                )));
            }
            "bybit" => bybit::push_sources(&mut out, &ctx),
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
            "bithumb" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BithumbSpotFeed::new(
                    spot_symbols.iter().map(|s| to_underscore(s)).collect(),
                )));
            }
            "bitmart" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(BitmartSpotFeed::new(
                        spot_symbols.iter().map(|s| to_underscore(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    let bitmart_perp_symbols = perp_symbols
                        .iter()
                        .map(|s| to_binance(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(BitmartPerpFeed::new(bitmart_perp_symbols.clone())));
                    out.push(Arc::new(BitmartPerpMetricsPoller::new(
                        bitmart_perp_symbols,
                    )));
                }
            }
            "bitmex" if !perp_symbols.is_empty() => {
                out.push(Arc::new(BitmexPerpFeed::new(
                    perp_symbols.iter().map(|s| to_bitmex_perp(s)).collect(),
                )));
            }
            "bitstamp" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BitstampSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
                )));
            }
            "bitvavo" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BitvavoSpotFeed::new(
                    spot_symbols.iter().map(|s| to_dash(s)).collect(),
                )));
            }
            "bitrue" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BitrueSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
                )));
            }
            "ascend_ex" if !spot_symbols.is_empty() => {
                out.push(Arc::new(AscendExSpotFeed::new(
                    spot_symbols.iter().map(|s| to_slash(s)).collect(),
                )));
            }
            "btc_markets" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BtcMarketsSpotFeed::new(
                    spot_symbols.iter().map(|s| to_dash(s)).collect(),
                )));
            }
            "bullish" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BullishSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
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
            "phemex" if !perp_symbols.is_empty() => {
                out.push(Arc::new(PhemexPerpFeed::new(
                    perp_symbols.iter().map(|s| to_phemex_perp(s)).collect(),
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
            "injective" if !spot_symbols.is_empty() || !perp_symbols.is_empty() => {
                out.push(Arc::new(InjectiveFeed::new(
                    spot_symbols.iter().map(|s| to_injective_spot(s)).collect(),
                    perp_symbols.iter().map(|s| to_injective_perp(s)).collect(),
                )));
            }
            "xrpl" if !spot_symbols.is_empty() => {
                let pairs = spot_symbols
                    .iter()
                    .filter_map(|symbol| to_xrpl_pair(symbol))
                    .collect::<Vec<_>>();
                if !pairs.is_empty() {
                    out.push(Arc::new(XrplSpotFeed::new(pairs)));
                }
            }
            "architect" if !perp_symbols.is_empty() => {
                out.push(Arc::new(ArchitectPerpFeed::new(
                    perp_symbols.iter().map(|s| to_architect_perp(s)).collect(),
                    exchange_api_key(cfg, &ex),
                )));
            }
            "decibel" if !perp_symbols.is_empty() => {
                out.push(Arc::new(DecibelPerpFeed::new(
                    perp_symbols.iter().map(|s| to_decibel_perp(s)).collect(),
                    exchange_api_key(cfg, &ex),
                )));
            }
            "deribit" if !perp_symbols.is_empty() => {
                out.push(Arc::new(DeribitPerpFeed::new(
                    perp_symbols.iter().map(|s| to_deribit_perp(s)).collect(),
                )));
            }
            "evedex" if !perp_symbols.is_empty() => {
                out.push(Arc::new(EvedexPerpFeed::new(
                    perp_symbols.iter().map(|s| to_evedex_perp(s)).collect(),
                )));
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
                let product_ids = spot_symbols.iter().map(|s| to_dash(s)).collect::<Vec<_>>();
                out.push(Arc::new(CoinbaseTicker::new(product_ids.clone())));
                out.push(Arc::new(CoinbaseRestFeed::new(product_ids)));
            }
            "coincheck" if !spot_symbols.is_empty() => {
                out.push(Arc::new(CoincheckSpotFeed::new(
                    spot_symbols
                        .iter()
                        .map(|s| to_underscore(s).to_ascii_lowercase())
                        .collect(),
                )));
            }
            "coinone" if !spot_symbols.is_empty() => {
                out.push(Arc::new(CoinoneSpotFeed::new(
                    spot_symbols
                        .iter()
                        .map(|s| to_underscore(s).to_ascii_lowercase())
                        .collect(),
                )));
            }
            "coinex" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(CoinexFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_coinex_market(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(CoinexFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_coinex_market(s)).collect(),
                    )));
                }
            }
            "upbit" if !spot_symbols.is_empty() => {
                out.push(Arc::new(UpbitSpotFeed::new(
                    spot_symbols.iter().map(|s| to_upbit_market(s)).collect(),
                )));
            }
            "woo" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(WooFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_woo_spot(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(WooFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_woo_perp(s)).collect(),
                    )));
                }
            }
            "gemini" if !spot_symbols.is_empty() => {
                out.push(Arc::new(GeminiSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
                )));
            }
            "cryptocom" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(CryptoComFeed::new(
                        crate::types::MarketKind::Spot,
                        spot_symbols.iter().map(|s| to_cryptocom_spot(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    out.push(Arc::new(CryptoComFeed::new(
                        crate::types::MarketKind::Perp,
                        perp_symbols.iter().map(|s| to_cryptocom_perp(s)).collect(),
                    )));
                }
            }
            "cube" if !spot_symbols.is_empty() => {
                out.push(Arc::new(CubeSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
                )));
            }
            "foxbit" if !spot_symbols.is_empty() => {
                out.push(Arc::new(FoxbitSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
                )));
            }
            "ndax" if !spot_symbols.is_empty() => {
                out.push(Arc::new(NdaxSpotFeed::new(
                    spot_symbols.iter().map(|s| to_binance(s)).collect(),
                )));
            }
            "kraken" => {
                if !spot_symbols.is_empty() {
                    out.push(Arc::new(KrakenTicker::new(
                        spot_symbols.iter().map(|s| to_slash(s)).collect(),
                    )));
                    out.push(Arc::new(KrakenRestFeed::new(
                        spot_symbols.iter().map(|s| to_binance(s)).collect(),
                    )));
                }
                if !perp_symbols.is_empty() {
                    let kraken_perp_symbols = perp_symbols
                        .iter()
                        .map(|s| to_kraken_perp(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(KrakenPerpTicker::new(kraken_perp_symbols.clone())));
                    out.push(Arc::new(KrakenPerpRestFeed::new(kraken_perp_symbols)));
                }
            }
            "kucoin" => {
                if !spot_symbols.is_empty() {
                    let spot_markets = spot_symbols.iter().map(|s| to_dash(s)).collect::<Vec<_>>();
                    out.push(Arc::new(KucoinTicker::new(spot_markets.clone())));
                    out.push(Arc::new(KucoinRestFeed::new(spot_markets)));
                }
                if !perp_symbols.is_empty() {
                    let kucoin_perp_symbols = perp_symbols
                        .iter()
                        .map(|s| to_kucoin_perp(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(KucoinPerpTicker::new(kucoin_perp_symbols.clone())));
                    out.push(Arc::new(KucoinPerpRestFeed::new(kucoin_perp_symbols)));
                }
            }
            "gate" => {
                if !spot_symbols.is_empty() {
                    let gate_spot_symbols = spot_symbols
                        .iter()
                        .map(|s| to_underscore(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(GateSpotBookTicker::new(gate_spot_symbols.clone())));
                    out.push(Arc::new(GateSpotRestFeed::new(gate_spot_symbols)));
                }
                if !perp_symbols.is_empty() {
                    let gate_perp_symbols = perp_symbols
                        .iter()
                        .map(|s| to_underscore(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(GatePerpBookTicker::new(gate_perp_symbols.clone())));
                    out.push(Arc::new(GatePerpRestFeed::new(gate_perp_symbols)));
                }
            }
            "binance" => {
                binance::push_sources(&mut out, &ctx);
            }
            "htx" => {
                if !spot_symbols.is_empty() {
                    let htx_spot_symbols = spot_symbols
                        .iter()
                        .map(|s| to_binance(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(HtxBbo::new(htx_spot_symbols.clone())));
                    out.push(Arc::new(HtxSpotRestFeed::new(htx_spot_symbols)));
                }
                if !perp_symbols.is_empty() {
                    let htx_perp_symbols = perp_symbols
                        .iter()
                        .map(|s| to_htx_perp(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(HtxPerpBbo::new(htx_perp_symbols.clone())));
                    out.push(Arc::new(HtxPerpRestFeed::new(htx_perp_symbols)));
                }
            }
            "bitfinex" => {
                if !spot_symbols.is_empty() {
                    let bitfinex_spot_symbols = spot_symbols
                        .iter()
                        .map(|s| to_bitfinex(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(BitfinexTicker::new(bitfinex_spot_symbols.clone())));
                    out.push(Arc::new(BitfinexSpotRestFeed::new(bitfinex_spot_symbols)));
                }
                if !perp_symbols.is_empty() {
                    let bitfinex_perp_symbols = perp_symbols
                        .iter()
                        .map(|s| to_bitfinex_perp(s))
                        .collect::<Vec<_>>();
                    out.push(Arc::new(BitfinexPerpTicker::new(
                        bitfinex_perp_symbols.clone(),
                    )));
                    out.push(Arc::new(BitfinexPerpRestFeed::new(bitfinex_perp_symbols)));
                }
            }
            "bitflyer" if !spot_symbols.is_empty() => {
                out.push(Arc::new(BitflyerSpotFeed::new(
                    spot_symbols.iter().map(|s| to_underscore(s)).collect(),
                )));
            }
            _ => {}
        }
    }

    let global_ctx = RegistryContext {
        cfg,
        spot_symbols: &[],
        perp_symbols: &[],
    };
    defi::push_sources(&mut out, &global_ctx);
    tradfi::push_sources(&mut out, &global_ctx);
    aggregate::push_sources(&mut out, &global_ctx);
    sentiment::push_sources(&mut out, &global_ctx);

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

fn to_cryptocom_spot(symbol: &str) -> String {
    if symbol.contains('_') {
        return symbol.to_ascii_uppercase();
    }
    let (base, quote) = split_quote(symbol);
    format!(
        "{}_{}",
        base.to_ascii_uppercase(),
        quote.to_ascii_uppercase()
    )
}

fn to_coinex_market(symbol: &str) -> String {
    symbol
        .to_ascii_uppercase()
        .replace("-PERP", "")
        .replace("_PERP", "")
        .replace("PERP", "")
        .replace(['-', '_', '/'], "")
}

fn to_phemex_perp(symbol: &str) -> String {
    symbol
        .to_ascii_uppercase()
        .replace("-PERP", "")
        .replace("_PERP", "")
        .replace("PERP", "")
        .replace(['-', '_', '/'], "")
}

fn to_upbit_market(symbol: &str) -> String {
    if symbol.contains('-') {
        return symbol.to_ascii_uppercase();
    }
    let compact = symbol.replace(['_', '/'], "").to_ascii_uppercase();
    let (base, quote) = split_quote(&compact);
    format!("{quote}-{base}")
}

fn to_woo_spot(symbol: &str) -> String {
    let (base, quote) = split_quote(symbol);
    format!(
        "SPOT_{}_{}",
        base.to_ascii_uppercase(),
        quote.to_ascii_uppercase()
    )
}

fn to_woo_perp(symbol: &str) -> String {
    let normalized = symbol
        .to_ascii_uppercase()
        .replace("-PERP", "")
        .replace("_PERP", "")
        .replace("PERP", "")
        .replace(['-', '_', '/'], "");
    let (base, quote) = split_quote(&normalized);
    format!(
        "PERP_{}_{}",
        base.to_ascii_uppercase(),
        quote.to_ascii_uppercase()
    )
}

fn to_cryptocom_perp(symbol: &str) -> String {
    if symbol.contains('-') {
        return symbol.to_ascii_uppercase();
    }
    if let Some(base) = symbol.strip_suffix("PERP") {
        return format!("{}USD-PERP", split_quote(base).0.to_ascii_uppercase());
    }
    let (base, _) = split_quote(symbol);
    format!("{}USD-PERP", base.to_ascii_uppercase())
}

fn to_bitmex_perp(symbol: &str) -> String {
    let upper = symbol.to_ascii_uppercase();
    if upper.starts_with("XBT") || upper.ends_with("USD") {
        return upper;
    }
    if let Some(base) = upper.strip_suffix("PERP") {
        return format!("{}USD", bitmex_base(base));
    }
    let (base, _) = split_quote(&upper);
    format!("{}USD", bitmex_base(base))
}

fn bitmex_base(base: &str) -> String {
    if base == "BTC" {
        "XBT".to_string()
    } else {
        base.to_ascii_uppercase()
    }
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
        return format!("{base}_{quote}_Perp");
    }
    let upper = symbol.to_ascii_uppercase();
    let (base, quote) = split_quote(&upper);
    format!("{base}_{quote}_Perp")
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

fn to_injective_spot(symbol: &str) -> String {
    to_slash(symbol)
}

fn to_injective_perp(symbol: &str) -> String {
    let (base, quote) = split_quote(symbol);
    format!("{base}/{quote}-PERP")
}

fn to_xrpl_pair(symbol: &str) -> Option<XrplPair> {
    let compact = compact_symbol(symbol);
    match compact.as_str() {
        "XRPUSD" => Some(XrplPair::xrp_usd("XRPUSD")),
        _ => None,
    }
}

fn to_architect_perp(symbol: &str) -> String {
    if symbol.contains('-') {
        return symbol.to_ascii_uppercase();
    }
    if let Some(base) = symbol.strip_suffix("PERP") {
        return format!("{}-PERP", base.to_ascii_uppercase());
    }
    let (base, _) = split_quote(symbol);
    format!("{}-PERP", base.to_ascii_uppercase())
}

fn to_decibel_perp(symbol: &str) -> String {
    if symbol.starts_with("0x") || symbol.contains('@') {
        return symbol.to_string();
    }
    if symbol.contains('/') {
        return symbol.to_ascii_uppercase().replace('/', "-");
    }
    if symbol.contains('-') {
        return symbol.to_ascii_uppercase();
    }
    let (base, _) = split_quote(symbol);
    format!("{}-USD", base.to_ascii_uppercase())
}

fn to_deribit_perp(symbol: &str) -> String {
    let upper = symbol.to_ascii_uppercase();
    if upper.contains("-PERPETUAL") {
        return upper;
    }
    let (base, _) = split_quote(&upper);
    format!("{base}-PERPETUAL")
}

fn to_evedex_perp(symbol: &str) -> String {
    let upper = symbol.to_ascii_uppercase();
    if upper.ends_with("USD") && !upper.ends_with("USDT") && !upper.contains('-') {
        return upper;
    }
    if upper.contains('-') || upper.contains('/') {
        return upper.replace(['-', '/'], "");
    }
    if let Some(base) = upper.strip_suffix("PERP") {
        return format!("{base}USD");
    }
    let (base, _) = split_quote(&upper);
    format!("{base}USD")
}

fn exchange_api_key(cfg: &AppConfig, exchange: &str) -> Option<String> {
    let ex = cfg.exchanges.get(exchange)?;
    ex.api_key.clone().or_else(|| {
        ex.api_key_env
            .as_ref()
            .and_then(|env| std::env::var(env).ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AggregatesConfig, AppConfig, BinanceOptionsConfig, BybitOptionsConfig, ClickHouseConfig,
        DefiConfig, DeribitConfig, ExchangeConfig, KlineConfig, OkxOptionsConfig, OnchainConfig,
        PolymarketConfig, RuntimeConfig, SentimentConfig, StrategyConfig, TradfiConfig,
        fees::FeeModel, runtime::BackpressureConfig,
    };
    use std::collections::HashMap;

    #[test]
    fn build_sources_creates_enabled_spot_and_perp_adapters() {
        let cfg = AppConfig {
            runtime: RuntimeConfig {
                queue_capacity: 16,
                router_publish_queue_capacity: 16,
                broadcast_capacity: 16,
                event_bus_shards: 1,
                backpressure: BackpressureConfig::Block,
                report_interval_ms: 1000,
                stale_ttl_ms: 1000,
                api_addr: "127.0.0.1:0".to_string(),
                api_key_env: None,
                api_key: None,
                api_rate_limit_per_minute: 0,
                redis_url: None,
                redis_stream_prefix: "ticks".to_string(),
                redis_dead_letter_path: "data/test_redis_dead_letters.jsonl".to_string(),
                clickhouse: ClickHouseConfig::default(),
                order_flow_large_trade_notional_usdt: 100_000.0,
                ws_send_timeout_ms: 3_000,
            },
            strategy: StrategyConfig {
                min_profit_usdt: 1.0,
                min_profit_bps: 1.0,
                min_signal_hold_ms: 0,
                slippage_bps: 0.0,
                fee_mode: crate::config::StrategyFeeMode::Taker,
                book_signal_notional_usdt: 1_000.0,
                fallback_maker_fee_bps: 2.0,
                fallback_taker_fee_bps: 10.0,
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
                    api_key: None,
                    api_key_env: None,
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

    #[test]
    fn decibel_perp_symbol_converter_uses_market_name_or_direct_address() {
        assert_eq!(to_decibel_perp("BTCUSDT"), "BTC-USD");
        assert_eq!(to_decibel_perp("ETH/USD"), "ETH-USD");
        assert_eq!(to_decibel_perp("BTC-USD@0xabc"), "BTC-USD@0xabc");
    }

    #[test]
    fn evedex_perp_symbol_converter_uses_compact_usd_symbol() {
        assert_eq!(to_evedex_perp("BTCUSDT"), "BTCUSD");
        assert_eq!(to_evedex_perp("ETH-USD"), "ETHUSD");
        assert_eq!(to_evedex_perp("SOLPERP"), "SOLUSD");
    }

    #[test]
    fn cryptocom_symbol_converters_use_exchange_ids() {
        assert_eq!(to_cryptocom_spot("BTCUSDT"), "BTC_USDT");
        assert_eq!(to_cryptocom_spot("ETH_USDT"), "ETH_USDT");
        assert_eq!(to_cryptocom_perp("BTCUSDT"), "BTCUSD-PERP");
        assert_eq!(to_cryptocom_perp("ETHUSD-PERP"), "ETHUSD-PERP");
    }

    #[test]
    fn coinex_symbol_converter_uses_compact_ids() {
        assert_eq!(to_coinex_market("BTCUSDT"), "BTCUSDT");
        assert_eq!(to_coinex_market("BTC-USDT"), "BTCUSDT");
        assert_eq!(to_coinex_market("BTC_USDT"), "BTCUSDT");
        assert_eq!(to_coinex_market("BTCUSDT-PERP"), "BTCUSDT");
    }

    #[test]
    fn phemex_perp_symbol_converter_uses_linear_ids() {
        assert_eq!(to_phemex_perp("BTCUSDT"), "BTCUSDT");
        assert_eq!(to_phemex_perp("BTC-USDT"), "BTCUSDT");
        assert_eq!(to_phemex_perp("BTCUSDT-PERP"), "BTCUSDT");
    }

    #[test]
    fn upbit_symbol_converter_uses_quote_base_ids() {
        assert_eq!(to_upbit_market("BTCUSDT"), "USDT-BTC");
        assert_eq!(to_upbit_market("BTCKRW"), "KRW-BTC");
        assert_eq!(to_upbit_market("USDT-BTC"), "USDT-BTC");
    }

    #[test]
    fn woo_symbol_converters_use_product_prefixed_ids() {
        assert_eq!(to_woo_spot("BTCUSDT"), "SPOT_BTC_USDT");
        assert_eq!(to_woo_perp("BTCUSDT"), "PERP_BTC_USDT");
        assert_eq!(to_woo_perp("BTC-USDT-PERP"), "PERP_BTC_USDT");
    }

    #[test]
    fn bitmex_perp_symbol_converter_uses_inverse_ids() {
        assert_eq!(to_bitmex_perp("BTCUSDT"), "XBTUSD");
        assert_eq!(to_bitmex_perp("XBTUSD"), "XBTUSD");
        assert_eq!(to_bitmex_perp("ETHUSDT"), "ETHUSD");
    }
}
