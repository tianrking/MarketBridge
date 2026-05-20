use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::DeribitConfig;
use crate::external::{DeribitOptionSummary, fetch_deribit_option_summaries_from};
use crate::types::now_ms;

#[derive(Debug, Clone, Serialize)]
pub struct CachedDeribitOptionSummary {
    pub version: &'static str,
    pub source: &'static str,
    pub received_at_ms: u64,
    pub stale: bool,
    #[serde(flatten)]
    pub summary: DeribitOptionSummary,
}

#[derive(Debug, Clone, Default)]
pub struct DeribitOptionFilter {
    pub currency: Option<String>,
    pub option_type: Option<String>,
    pub strike_min: Option<f64>,
    pub strike_max: Option<f64>,
    pub expiry_after: Option<String>,
    pub expiry_before: Option<String>,
    pub include_stale: bool,
}

#[derive(Clone)]
pub struct DeribitOptionCache {
    inner: Arc<RwLock<Vec<CachedDeribitOptionSummary>>>,
    stale_ttl_ms: u64,
}

impl DeribitOptionCache {
    pub fn new(stale_ttl_ms: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Vec::new())),
            stale_ttl_ms,
        }
    }

    pub async fn replace_currency(&self, currency: &str, rows: Vec<DeribitOptionSummary>) {
        let currency = currency.trim().to_ascii_uppercase();
        let received_at_ms = now_ms();
        let mut guard = self.inner.write().await;
        guard.retain(|row| row.summary.currency != currency);
        guard.extend(rows.into_iter().map(|summary| CachedDeribitOptionSummary {
            version: "v1",
            source: "deribit_rest_cache",
            received_at_ms,
            stale: false,
            summary,
        }));
    }

    pub async fn filtered(&self, filter: DeribitOptionFilter) -> Vec<CachedDeribitOptionSummary> {
        let currency = filter.currency.map(|x| x.trim().to_ascii_uppercase());
        let option_type = filter.option_type.map(|x| x.trim().to_ascii_lowercase());
        let guard = self.inner.read().await;
        guard
            .iter()
            .cloned()
            .map(|row| self.with_stale(row))
            .filter(|row| filter.include_stale || !row.stale)
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

    fn with_stale(&self, mut row: CachedDeribitOptionSummary) -> CachedDeribitOptionSummary {
        row.stale = now_ms().saturating_sub(row.received_at_ms) > self.stale_ttl_ms;
        row
    }
}

pub fn spawn_deribit_option_cache(
    cfg: DeribitConfig,
    client: reqwest::Client,
    cache: DeribitOptionCache,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let currencies = cfg
            .currencies
            .iter()
            .map(|x| x.trim().to_ascii_uppercase())
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>();
        if currencies.is_empty() {
            warn!("deribit option cache enabled with empty currencies");
            return;
        }
        loop {
            for currency in &currencies {
                match fetch_deribit_option_summaries_from(&client, &cfg.base_url, currency).await {
                    Ok(rows) => {
                        let count = rows.len();
                        cache.replace_currency(currency, rows).await;
                        info!(currency, count, "deribit option cache refreshed");
                    }
                    Err(error) => {
                        warn!(currency, %error, "deribit option cache refresh failed");
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(cfg.refresh_secs.max(1))).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(currency: &str, strike: f64, option_type: &str) -> DeribitOptionSummary {
        DeribitOptionSummary {
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
        let cache = DeribitOptionCache::new(30_000);
        cache
            .replace_currency(
                "BTC",
                vec![row("BTC", 90_000.0, "call"), row("BTC", 120_000.0, "put")],
            )
            .await;
        let rows = cache
            .filtered(DeribitOptionFilter {
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
