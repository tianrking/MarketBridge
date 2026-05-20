use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::{JupiterConfig, SolanaQuotePair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_quote, parse_f64_str, quote_to_price};

pub struct JupiterQuotePoller {
    cfg: JupiterConfig,
    client: reqwest::Client,
}

impl JupiterQuotePoller {
    pub fn new(cfg: JupiterConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JupiterQuoteResponse {
    in_amount: String,
    out_amount: String,
}

#[async_trait]
impl ExchangeSource for JupiterQuotePoller {
    fn name(&self) -> &'static str {
        "jupiter"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                match fetch_jupiter_price(&self.client, &self.cfg.base_url, pair).await {
                    Ok(price) => {
                        emit_defi_quote(&ctx, self.name(), &pair.symbol, price, pair.spread_bps)
                            .await?;
                    }
                    Err(error) => {
                        warn!(symbol=%pair.symbol, %error, "jupiter quote refresh failed");
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_jupiter_price(
    client: &reqwest::Client,
    base_url: &str,
    pair: &SolanaQuotePair,
) -> Result<f64> {
    let mut url = Url::parse(base_url)?.join("quote")?;
    url.query_pairs_mut()
        .append_pair("inputMint", &pair.input_mint)
        .append_pair("outputMint", &pair.output_mint)
        .append_pair("amount", &pair.amount.to_string())
        .append_pair("swapMode", "ExactIn");

    let quote = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<JupiterQuoteResponse>()
        .await
        .context("failed to decode jupiter quote")?;

    let in_amount = parse_f64_str(&quote.in_amount).context("invalid jupiter inAmount")?;
    let out_amount = parse_f64_str(&quote.out_amount).context("invalid jupiter outAmount")?;
    quote_to_price(
        in_amount,
        out_amount,
        pair.input_decimals,
        pair.output_decimals,
    )
    .context("invalid jupiter quote price")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jupiter_config_defaults_to_sol_usdc() {
        let cfg = JupiterConfig::default();
        assert_eq!(cfg.pairs[0].symbol, "SOLUSDC");
        assert_eq!(cfg.pairs[0].input_decimals, 9);
        assert_eq!(cfg.pairs[0].output_decimals, 6);
    }
}
