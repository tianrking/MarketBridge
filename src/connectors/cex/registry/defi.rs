use std::sync::Arc;

use crate::connectors::defi::dexscreener::DexScreenerPoller;
use crate::connectors::defi::jupiter::JupiterQuotePoller;
use crate::connectors::defi::oneinch::OneInchQuotePoller;
use crate::connectors::defi::paraswap::ParaswapQuotePoller;
use crate::connectors::defi::raydium::RaydiumPricePoller;
use crate::connectors::defi::uniswap_v3::UniswapV3PoolPoller;
use crate::source::ExchangeSource;

use super::RegistryContext;

pub(super) fn push_sources(out: &mut Vec<Arc<dyn ExchangeSource>>, ctx: &RegistryContext<'_>) {
    let cfg = ctx.cfg;
    if cfg.defi.jupiter.enabled {
        out.push(Arc::new(JupiterQuotePoller::new(cfg.defi.jupiter.clone())));
    }
    if cfg.defi.meteora.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "meteora",
            cfg.defi.meteora.clone(),
        )));
    }
    if cfg.defi.orca.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "orca",
            cfg.defi.orca.clone(),
        )));
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
    if cfg.defi.pancakeswap.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "pancakeswap",
            cfg.defi.pancakeswap.clone(),
        )));
    }
    if cfg.defi.balancer.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "balancer",
            cfg.defi.balancer.clone(),
        )));
    }
    if cfg.defi.curve.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "curve",
            cfg.defi.curve.clone(),
        )));
    }
    if cfg.defi.sushiswap.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "sushiswap",
            cfg.defi.sushiswap.clone(),
        )));
    }
    if cfg.defi.quickswap.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "quickswap",
            cfg.defi.quickswap.clone(),
        )));
    }
    if cfg.defi.traderjoe.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "traderjoe",
            cfg.defi.traderjoe.clone(),
        )));
    }
    if cfg.defi.etcswap.enabled {
        out.push(Arc::new(DexScreenerPoller::new(
            "etcswap",
            cfg.defi.etcswap.clone(),
        )));
    }
}
