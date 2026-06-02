use std::sync::Arc;

use crate::connectors::tradfi::fred::FredSeriesPoller;
use crate::connectors::tradfi::yahoo::YahooChartPoller;
use crate::source::ExchangeSource;

use super::RegistryContext;

pub(super) fn push_sources(out: &mut Vec<Arc<dyn ExchangeSource>>, ctx: &RegistryContext<'_>) {
    let cfg = ctx.cfg;
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
}
