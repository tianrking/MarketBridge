use std::sync::Arc;

use crate::source::ExchangeSource;

use super::super::bybit::{BybitDepthFeed, BybitLiquidationFeed, BybitSpotTicker, BybitTradeFeed};
use super::super::bybit_perp::BybitPerpTicker;
use super::RegistryContext;
use super::symbols::to_binance;

pub(super) fn push_sources(out: &mut Vec<Arc<dyn ExchangeSource>>, ctx: &RegistryContext<'_>) {
    if !ctx.spot_symbols.is_empty() {
        let spot = ctx
            .spot_symbols
            .iter()
            .map(|s| to_binance(s))
            .collect::<Vec<_>>();
        out.push(Arc::new(BybitSpotTicker::new(spot.clone())));
        out.push(Arc::new(BybitDepthFeed::new(
            crate::types::MarketKind::Spot,
            spot.clone(),
        )));
        out.push(Arc::new(BybitTradeFeed::new(
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
        out.push(Arc::new(BybitPerpTicker::new(perp.clone())));
        out.push(Arc::new(BybitLiquidationFeed::new(perp.clone())));
        out.push(Arc::new(BybitDepthFeed::new(
            crate::types::MarketKind::Perp,
            perp.clone(),
        )));
        out.push(Arc::new(BybitTradeFeed::new(
            crate::types::MarketKind::Perp,
            perp,
        )));
    }
}
