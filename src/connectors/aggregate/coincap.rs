use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde_json::Value;
use tracing::warn;

use crate::config::{CoinCapConfig, CoinPriceAsset};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{configured_api_key, emit_price_quote, parse_f64_value};

pub struct CoinCapPricePoller {
    cfg: CoinCapConfig,
    client: reqwest::Client,
}

impl CoinCapPricePoller {
    pub fn new(cfg: CoinCapConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CoinCapPricePoller {
    fn name(&self) -> &'static str {
        "coincap"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_assets(&self.client, &self.cfg).await {
                Ok(payload) => {
                    for asset in &self.cfg.assets {
                        if let Some(price) = asset_price(&payload, asset) {
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
                Err(error) => warn!(%error, "coincap price refresh failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_assets(client: &reqwest::Client, cfg: &CoinCapConfig) -> Result<Value> {
    let ids = cfg
        .assets
        .iter()
        .map(|asset| asset.id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let mut url = Url::parse(&cfg.base_url)?.join("assets")?;
    url.query_pairs_mut().append_pair("ids", &ids);

    let mut request = client.get(url);
    if let Some(api_key) = configured_api_key(&cfg.api_key, &cfg.api_key_env) {
        request = request.header("Authorization", api_key);
    }

    request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to decode coincap assets")
}

fn asset_price(payload: &Value, asset: &CoinPriceAsset) -> Option<f64> {
    payload
        .get("data")?
        .as_array()?
        .iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(asset.id.as_str()))
        .and_then(|row| row.get("priceUsd"))
        .and_then(parse_f64_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_coincap_asset_price() {
        let asset = CoinPriceAsset {
            symbol: "BTCUSD".to_string(),
            id: "bitcoin".to_string(),
            vs_currency: "usd".to_string(),
        };
        let payload = serde_json::json!({
            "data": [
                {"id": "ethereum", "priceUsd": "3000"},
                {"id": "bitcoin", "priceUsd": "100000"}
            ]
        });

        assert_eq!(asset_price(&payload, &asset), Some(100000.0));
    }
}
