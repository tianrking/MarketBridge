use std::sync::Arc;

use crate::source::ExchangeSource;

use super::super::binance::{
    BinanceBookTicker, BinanceDepthFeed, BinanceFundingTicker, BinanceLiquidationFeed,
    BinanceOpenInterestPoller, BinanceTradeFeed,
};
use super::super::binance_perp::BinancePerpBookTicker;
use super::RegistryContext;
use super::symbols::to_binance;

pub(super) fn push_sources(out: &mut Vec<Arc<dyn ExchangeSource>>, ctx: &RegistryContext<'_>) {
    if !ctx.spot_symbols.is_empty() {
        let spot = ctx
            .spot_symbols
            .iter()
            .map(|s| to_binance(s))
            .collect::<Vec<_>>();
        out.push(Arc::new(BinanceBookTicker::new(spot.clone())));
        out.push(Arc::new(BinanceDepthFeed::new(
            crate::types::MarketKind::Spot,
            spot.clone(),
        )));
        out.push(Arc::new(BinanceTradeFeed::new(
            crate::types::MarketKind::Spot,
            spot,
        )));
    }
    if !ctx.perp_symbols.is_empty() {
        let perp = ctx
            .perp_symbols
            .iter()
            .map(|s| to_binance(s))
            .collect::<Vec<_>>();
        out.push(Arc::new(BinancePerpBookTicker::new(perp.clone())));
        out.push(Arc::new(BinanceFundingTicker::new(perp.clone())));
        out.push(Arc::new(BinanceOpenInterestPoller::new(perp.clone())));
        out.push(Arc::new(BinanceLiquidationFeed::new(perp.clone())));
        out.push(Arc::new(BinanceDepthFeed::new(
            crate::types::MarketKind::Perp,
            perp.clone(),
        )));
        out.push(Arc::new(BinanceTradeFeed::new(
            crate::types::MarketKind::Perp,
            perp,
        )));
    }
}
