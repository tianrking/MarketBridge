use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde_json::Value;
use tracing::warn;

use crate::config::CoinMarketCapConfig;
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_external_signal, emit_price_quote, parse_f64_value, require_api_key};

pub struct CoinMarketCapPricePoller {
    cfg: CoinMarketCapConfig,
    client: reqwest::Client,
}

impl CoinMarketCapPricePoller {
    pub fn new(cfg: CoinMarketCapConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CoinMarketCapPricePoller {
    fn name(&self) -> &'static str {
        "coinmarketcap"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_quotes(&self.client, &self.cfg).await {
                Ok(quotes) => {
                    for symbol in &self.cfg.symbols {
                        if let Some(price) = symbol_price(&quotes, symbol) {
                            emit_price_quote(
                                &ctx,
                                self.name(),
                                &format!("{symbol}USD"),
                                price,
                                self.cfg.spread_bps,
                            )
                            .await?;
                        }
                        emit_symbol_metrics(&ctx, self.name(), &quotes, symbol).await?;
                    }
                }
                Err(error) => warn!(%error, "coinmarketcap quote refresh failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_quotes(client: &reqwest::Client, cfg: &CoinMarketCapConfig) -> Result<Value> {
    let api_key = require_api_key(&cfg.api_key, &cfg.api_key_env)?;
    let mut url = Url::parse(&cfg.base_url)?.join("cryptocurrency/quotes/latest")?;
    url.query_pairs_mut()
        .append_pair("symbol", &cfg.symbols.join(","))
        .append_pair("convert", "USD");

    client
        .get(url)
        .header("X-CMC_PRO_API_KEY", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to decode coinmarketcap quotes")
}

fn symbol_price(quotes: &Value, symbol: &str) -> Option<f64> {
    let data = quotes.get("data")?.get(symbol)?;
    let row = data
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(data);
    row.get("quote")?
        .get("USD")?
        .get("price")
        .and_then(parse_f64_value)
}

async fn emit_symbol_metrics(
    ctx: &SourceContext,
    source: &'static str,
    quotes: &Value,
    symbol: &str,
) -> Result<()> {
    for (metric, value) in [
        (
            "market_cap",
            symbol_quote_metric(quotes, symbol, "market_cap"),
        ),
        (
            "volume_24h",
            symbol_quote_metric(quotes, symbol, "volume_24h"),
        ),
        (
            "change_24h_pct",
            symbol_quote_metric(quotes, symbol, "percent_change_24h"),
        ),
    ] {
        if let Some(value) = value {
            emit_external_signal(
                ctx,
                source,
                "market_reference",
                Some(&format!("{symbol}USD")),
                metric,
                Some(value),
                None,
            )
            .await?;
        }
    }
    Ok(())
}

fn symbol_quote_metric(quotes: &Value, symbol: &str, metric: &str) -> Option<f64> {
    let data = quotes.get("data")?.get(symbol)?;
    let row = data
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(data);
    row.get("quote")?
        .get("USD")?
        .get(metric)
        .and_then(parse_f64_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_coinmarketcap_price() {
        let value = serde_json::json!({"data":{"BTC":[{"quote":{"USD":{"price":100.0}}}]}});
        assert_eq!(symbol_price(&value, "BTC"), Some(100.0));
    }

    #[test]
    fn extracts_coinmarketcap_market_metrics() {
        let value = serde_json::json!({
            "data": {
                "BTC": [{
                    "quote": {
                        "USD": {
                            "market_cap": 1_000_000.0,
                            "volume_24h": 50_000.0,
                            "percent_change_24h": 2.5
                        }
                    }
                }]
            }
        });
        assert_eq!(
            symbol_quote_metric(&value, "BTC", "market_cap"),
            Some(1_000_000.0)
        );
        assert_eq!(
            symbol_quote_metric(&value, "BTC", "volume_24h"),
            Some(50_000.0)
        );
        assert_eq!(
            symbol_quote_metric(&value, "BTC", "percent_change_24h"),
            Some(2.5)
        );
    }
}
