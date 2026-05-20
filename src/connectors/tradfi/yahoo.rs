use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::YahooIndicatorConfig;
use crate::source::{ExchangeSource, SourceContext};

use super::common::emit_tradfi_quote;

pub struct YahooChartPoller {
    source: &'static str,
    cfg: YahooIndicatorConfig,
    client: reqwest::Client,
}

impl YahooChartPoller {
    pub fn new(source: &'static str, cfg: YahooIndicatorConfig) -> Self {
        Self {
            source,
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooChartResult>>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooChartResult {
    meta: YahooMeta,
    indicators: YahooIndicators,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooMeta {
    regular_market_price: Option<f64>,
    chart_previous_close: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    close: Option<Vec<Option<f64>>>,
}

#[async_trait]
impl ExchangeSource for YahooChartPoller {
    fn name(&self) -> &'static str {
        self.source
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_yahoo_price(&self.client, &self.cfg).await {
                Ok(price) => {
                    emit_tradfi_quote(
                        &ctx,
                        self.name(),
                        &self.cfg.symbol,
                        price,
                        self.cfg.spread_bps,
                    )
                    .await?;
                }
                Err(error) => {
                    warn!(source=self.source, symbol=%self.cfg.symbol, yahoo_symbol=%self.cfg.yahoo_symbol, %error, "yahoo chart refresh failed");
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_yahoo_price(client: &reqwest::Client, cfg: &YahooIndicatorConfig) -> Result<f64> {
    let mut url = Url::parse(&cfg.base_url)?.join(&cfg.yahoo_symbol)?;
    url.query_pairs_mut()
        .append_pair("range", "1d")
        .append_pair("interval", "1m");

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<YahooChartResponse>()
        .await
        .context("failed to decode yahoo chart response")?;

    if let Some(error) = response.chart.error {
        anyhow::bail!("yahoo chart error: {error}");
    }

    let result = response
        .chart
        .result
        .and_then(|mut rows| rows.pop())
        .context("yahoo chart returned no result")?;

    latest_price(&result).context("yahoo chart returned no usable price")
}

fn latest_price(result: &YahooChartResult) -> Option<f64> {
    result
        .meta
        .regular_market_price
        .or(result.meta.chart_previous_close)
        .or_else(|| {
            result
                .indicators
                .quote
                .first()
                .and_then(|quote| quote.close.as_ref())
                .and_then(|closes| closes.iter().rev().flatten().copied().find(|x| *x > 0.0))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_price_prefers_regular_market_price() {
        let result = YahooChartResult {
            meta: YahooMeta {
                regular_market_price: Some(101.0),
                chart_previous_close: Some(99.0),
            },
            indicators: YahooIndicators {
                quote: vec![YahooQuote {
                    close: Some(vec![Some(100.0)]),
                }],
            },
        };
        assert_eq!(latest_price(&result), Some(101.0));
    }

    #[test]
    fn latest_price_falls_back_to_last_close() {
        let result = YahooChartResult {
            meta: YahooMeta {
                regular_market_price: None,
                chart_previous_close: None,
            },
            indicators: YahooIndicators {
                quote: vec![YahooQuote {
                    close: Some(vec![Some(1.0), None, Some(2.0)]),
                }],
            },
        };
        assert_eq!(latest_price(&result), Some(2.0));
    }
}
