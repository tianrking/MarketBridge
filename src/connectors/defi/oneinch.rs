use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::{EvmQuotePair, OneInchConfig};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_quote, parse_f64_str, quote_to_price};

pub struct OneInchQuotePoller {
    cfg: OneInchConfig,
    client: reqwest::Client,
}

impl OneInchQuotePoller {
    pub fn new(cfg: OneInchConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OneInchQuoteResponse {
    from_token_amount: Option<String>,
    to_token_amount: String,
}

#[async_trait]
impl ExchangeSource for OneInchQuotePoller {
    fn name(&self) -> &'static str {
        "oneinch"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                match fetch_oneinch_price(&self.client, &self.cfg.base_url, pair).await {
                    Ok(price) => {
                        emit_defi_quote(&ctx, self.name(), &pair.symbol, price, pair.spread_bps)
                            .await?;
                    }
                    Err(error) => {
                        warn!(symbol=%pair.symbol, network=pair.network, %error, "1inch quote refresh failed");
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_oneinch_price(
    client: &reqwest::Client,
    base_url: &str,
    pair: &EvmQuotePair,
) -> Result<f64> {
    let mut url = Url::parse(base_url)?.join(&format!("{}/quote", pair.network))?;
    url.query_pairs_mut()
        .append_pair("fromTokenAddress", &pair.src_token)
        .append_pair("toTokenAddress", &pair.dest_token)
        .append_pair("amount", &pair.amount);

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<OneInchQuoteResponse>()
        .await
        .context("failed to decode 1inch quote")?;

    quote_to_price(
        parse_f64_str(
            response
                .from_token_amount
                .as_deref()
                .unwrap_or(&pair.amount),
        )
        .context("invalid 1inch fromTokenAmount")?,
        parse_f64_str(&response.to_token_amount).context("invalid 1inch toTokenAmount")?,
        pair.src_decimals,
        pair.dest_decimals,
    )
    .context("invalid 1inch quote price")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oneinch_defaults_to_legacy_public_base_url() {
        let cfg = OneInchConfig::default();
        assert!(cfg.base_url.contains("1inch"));
    }
}
