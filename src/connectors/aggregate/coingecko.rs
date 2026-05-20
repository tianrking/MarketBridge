use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde_json::Value;
use tracing::warn;

use crate::config::{CoinGeckoConfig, CoinPriceAsset};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{configured_api_key, emit_price_quote, parse_f64_value};

pub struct CoinGeckoPricePoller {
    cfg: CoinGeckoConfig,
    client: reqwest::Client,
}

impl CoinGeckoPricePoller {
    pub fn new(cfg: CoinGeckoConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CoinGeckoPricePoller {
    fn name(&self) -> &'static str {
        "coingecko"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_prices(&self.client, &self.cfg).await {
                Ok(prices) => {
                    for asset in &self.cfg.assets {
                        if let Some(price) = asset_price(&prices, asset) {
                            emit_price_quote(
                                &ctx,
                                self.name(),
                                &asset.symbol,
                                price,
                                self.cfg.spread_bps,
                            )
                            .await?;
                        }
                    }
                }
                Err(error) => warn!(%error, "coingecko price refresh failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_prices(client: &reqwest::Client, cfg: &CoinGeckoConfig) -> Result<Value> {
    let ids = cfg
        .assets
        .iter()
        .map(|asset| asset.id.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    let vs_currencies = cfg
        .assets
        .iter()
        .map(|asset| asset.vs_currency.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");

    let mut url = Url::parse(&cfg.base_url)?.join("simple/price")?;
    url.query_pairs_mut()
        .append_pair("ids", &ids)
        .append_pair("vs_currencies", &vs_currencies)
        .append_pair("include_market_cap", "true")
        .append_pair("include_24hr_vol", "true")
        .append_pair("include_24hr_change", "true")
        .append_pair("include_last_updated_at", "true");

    let mut request = client.get(url);
    if let Some(api_key) = configured_api_key(&cfg.api_key, &cfg.api_key_env) {
        request = request.header("x-cg-demo-api-key", api_key.clone());
        request = request.header("x-cg-pro-api-key", api_key);
    }

    request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to decode coingecko prices")
}

fn asset_price(prices: &Value, asset: &CoinPriceAsset) -> Option<f64> {
    parse_f64_value(prices.get(&asset.id)?.get(&asset.vs_currency)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_asset_price() {
        let asset = CoinPriceAsset {
            symbol: "BTCUSD".to_string(),
            id: "bitcoin".to_string(),
            vs_currency: "usd".to_string(),
        };
        let prices = serde_json::json!({"bitcoin": {"usd": 100.0}});
        assert_eq!(asset_price(&prices, &asset), Some(100.0));
    }
}
