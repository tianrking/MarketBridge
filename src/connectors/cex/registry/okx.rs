use std::sync::Arc;

use crate::source::ExchangeSource;

use super::super::okx::{
    OkxDepthFeed, OkxFundingFeed, OkxLiquidationPoller, OkxOpenInterestFeed, OkxTicker,
    OkxTradeFeed,
};
use super::super::okx_perp::OkxPerpTicker;
use super::symbols::{to_okx, to_okx_swap};

pub(super) fn push_sources(
    out: &mut Vec<Arc<dyn ExchangeSource>>,
    spot_symbols: &[String],
    perp_symbols: &[String],
) {
    if !spot_symbols.is_empty() {
        let spot = spot_symbols.iter().map(|s| to_okx(s)).collect::<Vec<_>>();
        out.push(Arc::new(OkxTicker::new(spot.clone())));
        out.push(Arc::new(OkxDepthFeed::new(
            crate::types::MarketKind::Spot,
            spot.clone(),
        )));
        out.push(Arc::new(OkxTradeFeed::new(
            crate::types::MarketKind::Spot,
            spot,
        )));
    }
    if !perp_symbols.is_empty() {
        let perp = perp_symbols
            .iter()
            .map(|s| to_okx_swap(s))
            .collect::<Vec<_>>();
        out.push(Arc::new(OkxPerpTicker::new(perp.clone())));
        out.push(Arc::new(OkxFundingFeed::new(perp.clone())));
        out.push(Arc::new(OkxOpenInterestFeed::new(perp.clone())));
        out.push(Arc::new(OkxLiquidationPoller::new(perp.clone())));
        out.push(Arc::new(OkxDepthFeed::new(
            crate::types::MarketKind::Perp,
            perp.clone(),
        )));
        out.push(Arc::new(OkxTradeFeed::new(
            crate::types::MarketKind::Perp,
            perp,
        )));
    }
}
