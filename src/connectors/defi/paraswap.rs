use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::{EvmQuotePair, ParaswapConfig};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_quote, parse_f64_str, quote_to_price};

pub struct ParaswapQuotePoller {
    cfg: ParaswapConfig,
    client: reqwest::Client,
}

impl ParaswapQuotePoller {
    pub fn new(cfg: ParaswapConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParaswapResponse {
    price_route: ParaswapPriceRoute,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParaswapPriceRoute {
    src_amount: String,
    dest_amount: String,
}

#[async_trait]
impl ExchangeSource for ParaswapQuotePoller {
    fn name(&self) -> &'static str {
        "paraswap"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                match fetch_paraswap_price(&self.client, &self.cfg.base_url, pair).await {
                    Ok(price) => {
                        emit_defi_quote(&ctx, self.name(), &pair.symbol, price, pair.spread_bps)
                            .await?;
                    }
                    Err(error) => {
                        warn!(symbol=%pair.symbol, network=pair.network, %error, "paraswap quote refresh failed");
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_paraswap_price(
    client: &reqwest::Client,
    base_url: &str,
    pair: &EvmQuotePair,
) -> Result<f64> {
    let mut url = Url::parse(base_url)?.join("prices/")?;
    url.query_pairs_mut()
        .append_pair("srcToken", &pair.src_token)
        .append_pair("destToken", &pair.dest_token)
        .append_pair("amount", &pair.amount)
        .append_pair("srcDecimals", &pair.src_decimals.to_string())
        .append_pair("destDecimals", &pair.dest_decimals.to_string())
        .append_pair("side", "SELL")
        .append_pair("network", &pair.network.to_string());

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<ParaswapResponse>()
        .await
        .context("failed to decode paraswap price")?;

    quote_to_price(
        parse_f64_str(&response.price_route.src_amount).context("invalid paraswap srcAmount")?,
        parse_f64_str(&response.price_route.dest_amount).context("invalid paraswap destAmount")?,
        pair.src_decimals,
        pair.dest_decimals,
    )
    .context("invalid paraswap quote price")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paraswap_defaults_to_eth_usdc() {
        let cfg = ParaswapConfig::default();
        assert_eq!(cfg.pairs[0].symbol, "ETHUSDC");
        assert_eq!(cfg.pairs[0].network, 1);
    }
}
