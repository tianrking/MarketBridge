use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::warn;

use crate::config::{RaydiumConfig, RaydiumPair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::emit_defi_quote;

pub struct RaydiumPricePoller {
    cfg: RaydiumConfig,
    client: reqwest::Client,
}

impl RaydiumPricePoller {
    pub fn new(cfg: RaydiumConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for RaydiumPricePoller {
    fn name(&self) -> &'static str {
        "raydium"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_raydium_prices(&self.client, &self.cfg.price_url).await {
                Ok(prices) => {
                    for pair in &self.cfg.pairs {
                        match pair_price(&prices, pair) {
                            Some(price) => {
                                emit_defi_quote(
                                    &ctx,
                                    self.name(),
                                    &pair.symbol,
                                    price,
                                    pair.spread_bps,
                                )
                                .await?;
                            }
                            None => warn!(
                                symbol=%pair.symbol,
                                "raydium price pair missing base or quote mint"
                            ),
                        }
                    }
                }
                Err(error) => {
                    warn!(%error, "raydium price refresh failed");
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_raydium_prices(
    client: &reqwest::Client,
    price_url: &str,
) -> Result<HashMap<String, f64>> {
    client
        .get(price_url)
        .send()
        .await?
        .error_for_status()?
        .json::<HashMap<String, f64>>()
        .await
        .context("failed to decode raydium price map")
}

fn pair_price(prices: &HashMap<String, f64>, pair: &RaydiumPair) -> Option<f64> {
    let base = prices.get(&pair.base_mint)?;
    let quote = prices.get(&pair.quote_mint)?;
    if *base <= 0.0 || *quote <= 0.0 {
        return None;
    }
    Some(base / quote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_pair_price_from_usd_prices() {
        let pair = RaydiumPair {
            symbol: "SOLUSDC".to_string(),
            base_mint: "SOL".to_string(),
            quote_mint: "USDC".to_string(),
            spread_bps: 5.0,
        };
        let prices = HashMap::from([("SOL".to_string(), 150.0), ("USDC".to_string(), 1.0)]);
        assert_eq!(pair_price(&prices, &pair), Some(150.0));
    }
}
