use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use tracing::warn;

use crate::config::{SantimentConfig, SantimentMetric};
use crate::connectors::aggregate::common::{
    emit_external_signal, parse_f64_value, require_api_key,
};
use crate::source::{ExchangeSource, SourceContext};

pub struct SantimentPoller {
    cfg: SantimentConfig,
    client: reqwest::Client,
}

impl SantimentPoller {
    pub fn new(cfg: SantimentConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for SantimentPoller {
    fn name(&self) -> &'static str {
        "santiment"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match require_api_key(&self.cfg.api_key, &self.cfg.api_key_env) {
                Ok(api_key) => {
                    for metric in &self.cfg.metrics {
                        match fetch_metric(&self.client, &self.cfg.url, &api_key, metric).await {
                            Ok(raw) => {
                                emit_external_signal(
                                    &ctx,
                                    self.name(),
                                    "sentiment",
                                    Some(&metric.slug),
                                    &metric.metric,
                                    latest_metric_value(&raw),
                                    Some(raw),
                                )
                                .await?;
                            }
                            Err(error) => {
                                warn!(metric=%metric.metric, slug=%metric.slug, %error, "santiment metric refresh failed")
                            }
                        }
                    }
                }
                Err(error) => warn!(%error, "santiment api key missing"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_metric(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    metric: &SantimentMetric,
) -> Result<Value> {
    let body = json!({
        "query": "query Metric($metric: String!, $slug: String!, $from: DateTime!, $to: DateTime!, $interval: interval!) { getMetric(metric: $metric) { timeseriesDataJson(slug: $slug, from: $from, to: $to, interval: $interval) } }",
        "variables": {
            "metric": metric.metric,
            "slug": metric.slug,
            "from": metric.from,
            "to": metric.to,
            "interval": metric.interval
        }
    });
    client
        .post(url)
        .header("Authorization", format!("Apikey {api_key}"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to decode santiment graphql response")
}

fn latest_metric_value(raw: &Value) -> Option<f64> {
    raw.pointer("/data/getMetric/timeseriesDataJson")
        .and_then(|items| items.as_array())
        .and_then(|items| items.iter().rev().find_map(find_numeric_leaf))
}

fn find_numeric_leaf(value: &Value) -> Option<f64> {
    match value {
        Value::Number(_) | Value::String(_) => parse_f64_value(value),
        Value::Array(items) => items.iter().find_map(find_numeric_leaf),
        Value::Object(map) => map.values().find_map(find_numeric_leaf),
        _ => None,
    }
}
