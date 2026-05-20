use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::FredSeriesConfig;
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_tradfi_quote, parse_f64_str};

pub struct FredSeriesPoller {
    source: &'static str,
    cfg: FredSeriesConfig,
    client: reqwest::Client,
}

impl FredSeriesPoller {
    pub fn new(source: &'static str, cfg: FredSeriesConfig) -> Self {
        Self {
            source,
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct FredObservationsResponse {
    observations: Vec<FredObservation>,
}

#[derive(Debug, Deserialize)]
struct FredObservation {
    value: String,
}

#[async_trait]
impl ExchangeSource for FredSeriesPoller {
    fn name(&self) -> &'static str {
        self.source
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fred_api_key(&self.cfg) {
                Ok(api_key) => {
                    match fetch_fred_latest_value(&self.client, &self.cfg, api_key).await {
                        Ok(value) => {
                            emit_tradfi_quote(
                                &ctx,
                                self.name(),
                                &self.cfg.symbol,
                                value,
                                self.cfg.spread_bps,
                            )
                            .await?;
                        }
                        Err(error) => {
                            warn!(source=self.source, series=%self.cfg.series_id, %error, "fred series refresh failed");
                        }
                    }
                }
                Err(error) => {
                    warn!(source=self.source, series=%self.cfg.series_id, %error, "fred api key missing")
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_fred_latest_value(
    client: &reqwest::Client,
    cfg: &FredSeriesConfig,
    api_key: String,
) -> Result<f64> {
    let mut url = Url::parse(&cfg.base_url)?.join("series/observations")?;
    url.query_pairs_mut()
        .append_pair("series_id", &cfg.series_id)
        .append_pair("api_key", &api_key)
        .append_pair("file_type", "json")
        .append_pair("sort_order", "desc")
        .append_pair("limit", "5");

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<FredObservationsResponse>()
        .await
        .context("failed to decode fred observations")?;

    response
        .observations
        .iter()
        .find_map(|row| parse_f64_str(&row.value))
        .context("fred returned no numeric observations")
}

fn fred_api_key(cfg: &FredSeriesConfig) -> Result<String> {
    if let Some(api_key) = cfg
        .api_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(api_key.trim().to_string());
    }
    std::env::var(&cfg.api_key_env)
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .context("set tradfi.us10y.api_key or FRED_API_KEY")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fred_latest_value_skips_missing_observations() {
        let response = FredObservationsResponse {
            observations: vec![
                FredObservation {
                    value: ".".to_string(),
                },
                FredObservation {
                    value: "4.25".to_string(),
                },
            ],
        };
        let value = response
            .observations
            .iter()
            .find_map(|row| parse_f64_str(&row.value));
        assert_eq!(value, Some(4.25));
    }
}
