use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::config::{BinanceOptionsConfig, BybitOptionsConfig, DeribitConfig, OkxOptionsConfig};
use crate::connectors::options::binance::fetch_binance_option_summaries_from;
use crate::connectors::options::bybit::fetch_bybit_option_summaries_from;
use crate::connectors::options::common::OptionSummary;
use crate::connectors::options::deribit::fetch_deribit_option_summaries_from;
use crate::connectors::options::okx::fetch_okx_option_summaries_from;
use crate::types::now_ms;

#[derive(Debug, Clone, Serialize)]
pub struct CachedOptionSummary {
    pub version: &'static str,
    pub source: String,
    pub received_at_ms: u64,
    pub stale: bool,
    #[serde(flatten)]
    pub summary: OptionSummary,
}

#[derive(Debug, Clone, Default)]
pub struct OptionFilter {
    pub venue: Option<String>,
    pub currency: Option<String>,
    pub option_type: Option<String>,
    pub strike_min: Option<f64>,
    pub strike_max: Option<f64>,
    pub expiry_after: Option<String>,
    pub expiry_before: Option<String>,
    pub include_stale: bool,
}

#[derive(Clone)]
pub struct OptionCache {
    inner: Arc<RwLock<Vec<CachedOptionSummary>>>,
    stale_ttl_ms: u64,
}

impl OptionCache {
    pub fn new(stale_ttl_ms: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Vec::new())),
            stale_ttl_ms,
        }
    }

    pub async fn replace_venue_currency(
        &self,
        venue: &str,
        currency: &str,
        rows: Vec<OptionSummary>,
    ) {
        let venue = venue.trim().to_ascii_lowercase();
        let currency = currency.trim().to_ascii_uppercase();
        let received_at_ms = now_ms();
        let mut guard = self.inner.write().await;
        guard.retain(|row| {
            !(row.summary.venue.eq_ignore_ascii_case(&venue) && row.summary.currency == currency)
        });
        let source = format!("{venue}_rest_cache");
        guard.extend(rows.into_iter().map(|summary| CachedOptionSummary {
            version: "v1",
            source: source.clone(),
            received_at_ms,
            stale: false,
            summary,
        }));
    }

    pub async fn filtered(&self, filter: OptionFilter) -> Vec<CachedOptionSummary> {
        let venue = filter.venue.map(|x| x.trim().to_ascii_lowercase());
        let currency = filter.currency.map(|x| x.trim().to_ascii_uppercase());
        let option_type = filter.option_type.map(|x| x.trim().to_ascii_lowercase());
        let guard = self.inner.read().await;
        guard
            .iter()
            .cloned()
            .map(|row| self.with_stale(row))
            .filter(|row| filter.include_stale || !row.stale)
            .filter(|row| {
                venue
                    .as_ref()
                    .is_none_or(|value| row.summary.venue.eq_ignore_ascii_case(value))
            })
            .filter(|row| {
                currency
                    .as_ref()
                    .is_none_or(|value| row.summary.currency == *value)
            })
            .filter(|row| {
                option_type.as_ref().is_none_or(|value| {
                    row.summary
                        .option_type
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(value))
                })
            })
            .filter(|row| {
                filter
                    .strike_min
                    .is_none_or(|min| row.summary.strike.is_some_and(|x| x >= min))
            })
            .filter(|row| {
                filter
                    .strike_max
                    .is_none_or(|max| row.summary.strike.is_some_and(|x| x <= max))
            })
            .filter(|row| {
                filter.expiry_after.as_ref().is_none_or(|min| {
                    row.summary
                        .expiry_time
                        .as_deref()
                        .is_some_and(|x| x >= min.as_str())
                })
            })
            .filter(|row| {
                filter.expiry_before.as_ref().is_none_or(|max| {
                    row.summary
                        .expiry_time
                        .as_deref()
                        .is_some_and(|x| x <= max.as_str())
                })
            })
            .collect()
    }

    fn with_stale(&self, mut row: CachedOptionSummary) -> CachedOptionSummary {
        row.stale = now_ms().saturating_sub(row.received_at_ms) > self.stale_ttl_ms;
        row
    }
}

pub type DeribitOptionFilter = OptionFilter;
pub type DeribitOptionCache = OptionCache;

pub fn spawn_deribit_option_cache(
    cfg: DeribitConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let currencies = normalized_currencies(&cfg.currencies);
        if currencies.is_empty() {
            warn!("deribit option cache enabled with empty currencies");
            return;
        }
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            for currency in &currencies {
                match fetch_deribit_option_summaries_from(&client, &cfg.base_url, currency).await {
                    Ok(rows) => {
                        let count = rows.len();
                        cache
                            .replace_venue_currency("deribit", currency, rows)
                            .await;
                        info!(currency, count, "deribit option cache refreshed");
                    }
                    Err(error) => {
                        warn!(currency, %error, "deribit option cache refresh failed");
                    }
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(cfg.refresh_secs.max(1))) => {}
            }
        }
    })
}

pub fn spawn_okx_option_cache(
    cfg: OkxOptionsConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let currencies = normalized_currencies(&cfg.currencies);
        if currencies.is_empty() {
            warn!("okx option cache enabled with empty currencies");
            return;
        }
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            for currency in &currencies {
                match fetch_okx_option_summaries_from(&client, &cfg.base_url, currency).await {
                    Ok(rows) => {
                        let count = rows.len();
                        cache.replace_venue_currency("okx", currency, rows).await;
                        info!(currency, count, "okx option cache refreshed");
                    }
                    Err(error) => {
                        warn!(currency, %error, "okx option cache refresh failed");
                    }
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(cfg.refresh_secs.max(1))) => {}
            }
        }
    })
}

pub fn spawn_bybit_option_cache(
    cfg: BybitOptionsConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let currencies = normalized_currencies(&cfg.currencies);
        if currencies.is_empty() {
            warn!("bybit option cache enabled with empty currencies");
            return;
        }
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            for currency in &currencies {
                match fetch_bybit_option_summaries_from(&client, &cfg.base_url, currency).await {
                    Ok(rows) => {
                        let count = rows.len();
                        cache.replace_venue_currency("bybit", currency, rows).await;
                        info!(currency, count, "bybit option cache refreshed");
                    }
                    Err(error) => {
                        warn!(currency, %error, "bybit option cache refresh failed");
                    }
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(cfg.refresh_secs.max(1))) => {}
            }
        }
    })
}

pub fn spawn_binance_option_cache(
    cfg: BinanceOptionsConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let currencies = normalized_currencies(&cfg.currencies);
        if currencies.is_empty() {
            warn!("binance option cache enabled with empty currencies");
            return;
        }
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            for currency in &currencies {
                match fetch_binance_option_summaries_from(&client, &cfg.base_url, currency).await {
                    Ok(rows) => {
                        let count = rows.len();
                        cache
                            .replace_venue_currency("binance", currency, rows)
                            .await;
                        info!(currency, count, "binance option cache refreshed");
                    }
                    Err(error) => {
                        warn!(currency, %error, "binance option cache refresh failed");
                    }
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(cfg.refresh_secs.max(1))) => {}
            }
        }
    })
}

fn normalized_currencies(currencies: &[String]) -> Vec<String> {
    currencies
        .iter()
        .map(|x| x.trim().to_ascii_uppercase())
        .filter(|x| !x.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(currency: &str, strike: f64, option_type: &str) -> OptionSummary {
        OptionSummary {
            venue: "deribit".to_string(),
            currency: currency.to_string(),
            instrument_name: format!("{currency}-TEST-{strike}-{option_type}"),
            option_type: Some(option_type.to_string()),
            strike: Some(strike),
            expiry_time: Some("2026-12-25T08:00:00Z".to_string()),
            bid_price: Some(0.1),
            ask_price: Some(0.2),
            mark_price: Some(0.15),
            mark_iv: Some(55.0),
            underlying_price: Some(100_000.0),
            underlying_index: Some(currency.to_string()),
            open_interest: Some(1.0),
        }
    }

    #[tokio::test]
    async fn filters_cached_options() {
        let cache = OptionCache::new(30_000);
        cache
            .replace_venue_currency(
                "deribit",
                "BTC",
                vec![row("BTC", 90_000.0, "call"), row("BTC", 120_000.0, "put")],
            )
            .await;
        let rows = cache
            .filtered(OptionFilter {
                venue: Some("DERIBIT".to_string()),
                currency: Some("btc".to_string()),
                option_type: Some("CALL".to_string()),
                strike_min: Some(80_000.0),
                strike_max: Some(100_000.0),
                include_stale: false,
                ..Default::default()
            })
            .await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].summary.strike, Some(90_000.0));
    }
}
