use std::sync::Arc;

use crate::connectors::aggregate::coincap::CoinCapPricePoller;
use crate::connectors::aggregate::coingecko::CoinGeckoPricePoller;
use crate::connectors::aggregate::coinglass::CoinGlassPoller;
use crate::connectors::aggregate::coinmarketcap::CoinMarketCapPricePoller;
use crate::connectors::aggregate::custom_api::CustomApiPoller;
use crate::source::ExchangeSource;

use super::RegistryContext;

pub(super) fn push_sources(out: &mut Vec<Arc<dyn ExchangeSource>>, ctx: &RegistryContext<'_>) {
    let cfg = ctx.cfg;
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
}
