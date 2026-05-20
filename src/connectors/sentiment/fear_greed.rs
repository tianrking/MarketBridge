use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use tracing::warn;

use crate::config::FearGreedConfig;
use crate::connectors::aggregate::common::emit_external_signal;
use crate::source::{ExchangeSource, SourceContext};

pub struct FearGreedPoller {
    cfg: FearGreedConfig,
    client: reqwest::Client,
}

impl FearGreedPoller {
    pub fn new(cfg: FearGreedConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct FearGreedResponse {
    data: Vec<FearGreedRow>,
}

#[derive(Debug, Deserialize)]
struct FearGreedRow {
    value: String,
    value_classification: String,
}

#[async_trait]
impl ExchangeSource for FearGreedPoller {
    fn name(&self) -> &'static str {
        "fear_greed"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_fear_greed(&self.client, &self.cfg.url).await {
                Ok(row) => {
                    let value = row.value.parse::<f64>().ok();
                    emit_external_signal(
                        &ctx,
                        self.name(),
                        "sentiment",
                        Some("BTC"),
                        "fear_greed_index",
                        value,
                        Some(serde_json::json!({"classification": row.value_classification})),
                    )
                    .await?;
                }
                Err(error) => warn!(%error, "fear greed refresh failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_fear_greed(client: &reqwest::Client, url: &str) -> Result<FearGreedRow> {
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<FearGreedResponse>()
        .await
        .context("failed to decode fear greed index")?
        .data
        .into_iter()
        .next()
        .context("fear greed response is empty")
}
