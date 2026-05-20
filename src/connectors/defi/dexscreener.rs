use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::{DexScreenerConfig, DexScreenerPair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_quote, parse_f64_str};

pub struct DexScreenerPoller {
    source_name: &'static str,
    cfg: DexScreenerConfig,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    #[serde(default)]
    pairs: Vec<SearchPair>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchPair {
    #[serde(default)]
    chain_id: String,
    #[serde(default)]
    dex_id: String,
    price_usd: Option<String>,
    price_native: Option<String>,
}

impl DexScreenerPoller {
    pub fn new(source_name: &'static str, cfg: DexScreenerConfig) -> Self {
        Self {
            source_name,
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for DexScreenerPoller {
    fn name(&self) -> &'static str {
        self.source_name
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                match fetch_dexscreener_price(&self.client, &self.cfg.base_url, pair).await {
                    Ok(price) => {
                        emit_defi_quote(&ctx, self.name(), &pair.symbol, price, pair.spread_bps)
                            .await?;
                    }
                    Err(error) => warn!(
                        source = self.source_name,
                        symbol=%pair.symbol,
                        %error,
                        "dexscreener refresh failed"
                    ),
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_dexscreener_price(
    client: &reqwest::Client,
    base_url: &str,
    pair: &DexScreenerPair,
) -> Result<f64> {
    let mut url = Url::parse(base_url)?.join("latest/dex/search")?;
    url.query_pairs_mut().append_pair("q", &pair.query);
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<SearchResponse>()
        .await
        .context("failed to decode dexscreener search response")?;
    select_pair_price(&response, pair).context("matching dexscreener pair not found")
}

fn select_pair_price(response: &SearchResponse, pair: &DexScreenerPair) -> Option<f64> {
    response
        .pairs
        .iter()
        .find(|candidate| {
            candidate.chain_id.eq_ignore_ascii_case(&pair.chain_id)
                && candidate.dex_id.eq_ignore_ascii_case(&pair.dex_id)
        })
        .and_then(|candidate| {
            candidate
                .price_usd
                .as_deref()
                .or(candidate.price_native.as_deref())
                .and_then(parse_f64_str)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dexscreener_selects_matching_dex_pair_price() {
        let response = SearchResponse {
            pairs: vec![
                SearchPair {
                    chain_id: "solana".to_string(),
                    dex_id: "raydium".to_string(),
                    price_usd: Some("199".to_string()),
                    price_native: None,
                },
                SearchPair {
                    chain_id: "solana".to_string(),
                    dex_id: "meteora".to_string(),
                    price_usd: Some("200".to_string()),
                    price_native: None,
                },
            ],
        };
        let pair = DexScreenerPair {
            symbol: "SOLUSDC".to_string(),
            chain_id: "solana".to_string(),
            dex_id: "meteora".to_string(),
            query: "SOL USDC".to_string(),
            spread_bps: 5.0,
        };
        assert_eq!(select_pair_price(&response, &pair), Some(200.0));
    }
}
