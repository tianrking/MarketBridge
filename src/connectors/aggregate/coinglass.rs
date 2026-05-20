use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde_json::Value;
use tracing::warn;

use crate::config::CoinGlassConfig;
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_external_signal, parse_f64_value, require_api_key};

pub struct CoinGlassPoller {
    cfg: CoinGlassConfig,
    client: reqwest::Client,
}

impl CoinGlassPoller {
    pub fn new(cfg: CoinGlassConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CoinGlassEndpoint {
    metric: &'static str,
    category: &'static str,
    path: &'static str,
}

const ENDPOINTS: &[CoinGlassEndpoint] = &[
    CoinGlassEndpoint {
        metric: "funding_rate",
        category: "derivatives",
        path: "api/futures/fundingRate/exchange-list",
    },
    CoinGlassEndpoint {
        metric: "open_interest",
        category: "derivatives",
        path: "api/futures/openInterest/exchange-list",
    },
    CoinGlassEndpoint {
        metric: "liquidation",
        category: "derivatives",
        path: "api/futures/liquidation/aggregated-history",
    },
    CoinGlassEndpoint {
        metric: "long_short_ratio",
        category: "positioning",
        path: "api/futures/longShortRate/history",
    },
    CoinGlassEndpoint {
        metric: "basis",
        category: "basis",
        path: "api/futures/basis/history",
    },
    CoinGlassEndpoint {
        metric: "options_open_interest",
        category: "options",
        path: "api/option/openInterest/history",
    },
];

#[async_trait]
impl ExchangeSource for CoinGlassPoller {
    fn name(&self) -> &'static str {
        "coinglass"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match require_api_key(&self.cfg.api_key, &self.cfg.api_key_env) {
                Ok(api_key) => {
                    for symbol in &self.cfg.symbols {
                        for endpoint in ENDPOINTS {
                            match fetch_endpoint(
                                &self.client,
                                &self.cfg,
                                &api_key,
                                symbol,
                                endpoint,
                            )
                            .await
                            {
                                Ok(raw) => {
                                    let value = first_numeric_value(&raw);
                                    emit_external_signal(
                                        &ctx,
                                        self.name(),
                                        endpoint.category,
                                        Some(symbol),
                                        endpoint.metric,
                                        value,
                                        Some(raw),
                                    )
                                    .await?;
                                }
                                Err(error) => warn!(
                                    symbol=%symbol,
                                    metric=endpoint.metric,
                                    %error,
                                    "coinglass metric refresh failed"
                                ),
                            }
                        }
                    }
                }
                Err(error) => warn!(%error, "coinglass api key missing"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_endpoint(
    client: &reqwest::Client,
    cfg: &CoinGlassConfig,
    api_key: &str,
    symbol: &str,
    endpoint: &CoinGlassEndpoint,
) -> Result<Value> {
    let mut url = Url::parse(&cfg.base_url)?.join(endpoint.path)?;
    url.query_pairs_mut()
        .append_pair("symbol", symbol)
        .append_pair("interval", "1h")
        .append_pair("limit", "1");

    client
        .get(url)
        .header("CG-API-KEY", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .with_context(|| format!("failed to decode coinglass {}", endpoint.metric))
}

fn first_numeric_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(_) | Value::String(_) => parse_f64_value(value),
        Value::Array(items) => items.iter().find_map(first_numeric_value),
        Value::Object(map) => {
            for key in [
                "value",
                "rate",
                "fundingRate",
                "openInterest",
                "sumOpenInterest",
                "longShortRatio",
                "basis",
                "total",
                "volUsd",
            ] {
                if let Some(value) = map.get(key).and_then(parse_f64_value) {
                    return Some(value);
                }
            }
            map.values().find_map(first_numeric_value)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_first_numeric_metric_from_nested_payload() {
        let value = serde_json::json!({"data":[{"fundingRate":"0.0001"}]});
        assert_eq!(first_numeric_value(&value), Some(0.0001));
    }
}
