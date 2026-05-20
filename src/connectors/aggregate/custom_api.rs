use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tracing::warn;

use crate::config::CustomApiConfig;
use crate::connectors::aggregate::common::{emit_external_signal, parse_f64_value};
use crate::source::{ExchangeSource, SourceContext};

pub struct CustomApiPoller {
    cfg: CustomApiConfig,
    client: reqwest::Client,
}

impl CustomApiPoller {
    pub fn new(cfg: CustomApiConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CustomApiPoller {
    fn name(&self) -> &'static str {
        "custom_api"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_custom_value(&self.client, &self.cfg).await {
                Ok((value, raw)) => {
                    emit_external_signal(
                        &ctx,
                        self.name(),
                        &self.cfg.category,
                        self.cfg.symbol.as_deref(),
                        &self.cfg.metric,
                        value,
                        raw,
                    )
                    .await?;
                }
                Err(error) => warn!(source=%self.cfg.name, %error, "custom api refresh failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_custom_value(
    client: &reqwest::Client,
    cfg: &CustomApiConfig,
) -> Result<(Option<f64>, Option<Value>)> {
    let text = client
        .get(&cfg.url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
        .context("failed to read custom api response")?;

    if let Ok(value) = text.trim().parse::<f64>() {
        return Ok((Some(value), Some(Value::String(text))));
    }

    let raw = serde_json::from_str::<Value>(&text)
        .context("custom api response is not numeric or json")?;
    let value = value_at_path(&raw, &cfg.value_path).and_then(parse_f64_value);
    Ok((value, Some(raw)))
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.trim().is_empty() {
        return Some(value);
    }
    let mut current = value;
    for part in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(part)?,
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_api_value_path_reads_nested_json() {
        let raw = serde_json::json!({"data":[{"price":"123.45"}]});
        assert_eq!(
            value_at_path(&raw, "data.0.price").and_then(parse_f64_value),
            Some(123.45)
        );
    }
}
