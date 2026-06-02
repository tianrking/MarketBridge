use std::sync::Arc;

use crate::config::AppConfig;
use crate::connectors::sentiment::cryptopanic::CryptoPanicPoller;
use crate::connectors::sentiment::fear_greed::FearGreedPoller;
use crate::connectors::sentiment::lunarcrush::LunarCrushPoller;
use crate::connectors::sentiment::santiment::SantimentPoller;
use crate::source::ExchangeSource;

pub(super) fn push_sources(out: &mut Vec<Arc<dyn ExchangeSource>>, cfg: &AppConfig) {
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
}
