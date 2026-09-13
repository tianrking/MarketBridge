//! Explicit-identity circulating-supply reference snapshots.
//!
//! This module intentionally refuses ticker-only joins. An operator supplies
//! an asset ID, provider ID, explicit perp symbols and identity evidence in
//! configuration before a provider market-cap row may be attached to OI.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::sync::RwLock;

use crate::config::{SupplyAssetConfig, SupplyConfig};
use crate::types::now_ms;

#[derive(Debug, Clone, Serialize)]
pub struct SupplySnapshot {
    pub asset_id: String,
    pub provider: String,
    pub provider_asset_id: String,
    pub perp_symbols: Vec<String>,
    pub identity_evidence: String,
    pub chain: Option<String>,
    pub contract_address: Option<String>,
    pub circulating_supply: Option<f64>,
    pub price_usd: Option<f64>,
    pub circulating_market_cap_usd: Option<f64>,
    pub provider_last_updated: Option<String>,
    pub retrieved_at_ms: u64,
    pub identity_verified: bool,
    pub status: &'static str,
}

#[derive(Clone, Default)]
pub struct SupplySnapshotStore(Arc<RwLock<HashMap<String, SupplySnapshot>>>);

impl SupplySnapshotStore {
    #[cfg(test)]
    pub async fn replace_all(&self, rows: Vec<SupplySnapshot>) {
        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            map.insert(row.asset_id.clone(), row);
        }
        *self.0.write().await = map;
    }

    pub async fn all(&self) -> Vec<SupplySnapshot> {
        let mut rows = self.0.read().await.values().cloned().collect::<Vec<_>>();
        rows.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
        rows
    }

    /// Finds only a manually configured mapping. It never guesses an asset
    /// identity from the leading characters of a perpetual symbol.
    pub async fn for_perp_symbol(&self, symbol: &str) -> Option<SupplySnapshot> {
        let wanted = symbol.trim().to_ascii_uppercase();
        self.0
            .read()
            .await
            .values()
            .find(|row| {
                row.perp_symbols
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(&wanted))
            })
            .cloned()
    }

    pub async fn replace_for_assets(&self, rows: Vec<SupplySnapshot>) {
        let mut snapshots = self.0.write().await;
        for row in rows {
            snapshots.insert(row.asset_id.clone(), row);
        }
    }
}

/// Fetches supply reference data only when a caller asks for a configured
/// identity.  It intentionally has no background polling loop: enabling a
/// mapping authorizes the provider, but does not consume its quota.
#[derive(Clone)]
pub struct SupplyReferenceService {
    config: SupplyConfig,
    store: SupplySnapshotStore,
    client: reqwest::Client,
    refresh_lock: Arc<Mutex<()>>,
}

impl SupplyReferenceService {
    pub fn new(config: SupplyConfig, store: SupplySnapshotStore, client: reqwest::Client) -> Self {
        Self {
            config,
            store,
            client,
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn ensure_for_perp_symbol(&self, symbol: &str) -> Result<Option<SupplySnapshot>> {
        if !self.config.enabled {
            return Ok(None);
        }
        let wanted = symbol.trim().to_ascii_uppercase();
        let Some(asset) = self.config.assets.iter().find(|asset| {
            asset
                .perp_symbols
                .iter()
                .any(|item| item.eq_ignore_ascii_case(&wanted))
        }) else {
            return Ok(None);
        };
        if let Some(snapshot) = self.store.for_perp_symbol(&wanted).await
            && now_ms().saturating_sub(snapshot.retrieved_at_ms)
                < self.config.poll_secs.max(10).saturating_mul(1_000)
        {
            return Ok(Some(snapshot));
        }

        // A single lock prevents a burst of API readers from each spending a
        // provider request for the same stale mapping.
        let _guard = self.refresh_lock.lock().await;
        if let Some(snapshot) = self.store.for_perp_symbol(&wanted).await
            && now_ms().saturating_sub(snapshot.retrieved_at_ms)
                < self.config.poll_secs.max(10).saturating_mul(1_000)
        {
            return Ok(Some(snapshot));
        }
        let rows = fetch_coingecko(&self.client, &self.config, std::slice::from_ref(asset)).await?;
        self.store.replace_for_assets(rows).await;
        Ok(self.store.for_perp_symbol(&wanted).await)
    }
}

async fn fetch_coingecko(
    client: &reqwest::Client,
    config: &SupplyConfig,
    assets: &[SupplyAssetConfig],
) -> Result<Vec<SupplySnapshot>> {
    let provider_ids = assets
        .iter()
        .map(|asset| asset.provider_asset_id.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    let mut url = Url::parse(&config.base_url)?.join("coins/markets")?;
    url.query_pairs_mut()
        .append_pair("vs_currency", "usd")
        .append_pair("ids", &provider_ids)
        .append_pair("sparkline", "false")
        .append_pair("price_change_percentage", "24h");
    let mut request = client.get(url);
    if let Some(key) = config
        .api_key
        .clone()
        .or_else(|| std::env::var(&config.api_key_env).ok())
        .filter(|key| !key.trim().is_empty())
    {
        request = request
            .header("x-cg-demo-api-key", key.clone())
            .header("x-cg-pro-api-key", key);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to decode CoinGecko supply response")?;
    let rows = payload
        .as_array()
        .context("CoinGecko supply response must be an array")?;
    let lookup = rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(|id| (id, row)))
        .collect::<HashMap<_, _>>();
    let retrieved_at_ms = now_ms();
    Ok(assets
        .iter()
        .map(|asset| {
            snapshot_from_provider(
                asset,
                lookup.get(asset.provider_asset_id.as_str()).copied(),
                retrieved_at_ms,
            )
        })
        .collect())
}

fn snapshot_from_provider(
    asset: &SupplyAssetConfig,
    row: Option<&Value>,
    retrieved_at_ms: u64,
) -> SupplySnapshot {
    let value = |name: &str| row.and_then(|item| item.get(name)).and_then(parse_f64);
    let provider_last_updated = row
        .and_then(|item| item.get("last_updated"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let circulating_market_cap_usd = value("market_cap").filter(|item| *item > 0.0);
    SupplySnapshot {
        asset_id: asset.asset_id.clone(),
        provider: "coingecko".into(),
        provider_asset_id: asset.provider_asset_id.clone(),
        perp_symbols: asset
            .perp_symbols
            .iter()
            .map(|symbol| symbol.to_ascii_uppercase())
            .collect(),
        identity_evidence: asset.identity_evidence.clone(),
        chain: asset.chain.clone(),
        contract_address: asset.contract_address.clone(),
        circulating_supply: value("circulating_supply").filter(|item| *item >= 0.0),
        price_usd: value("current_price").filter(|item| *item > 0.0),
        circulating_market_cap_usd,
        provider_last_updated,
        retrieved_at_ms,
        identity_verified: true,
        status: if circulating_market_cap_usd.is_some() {
            "reference_available"
        } else {
            "provider_row_missing_or_incomplete"
        },
    }
}

fn parse_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .filter(|number: &f64| number.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> SupplyAssetConfig {
        SupplyAssetConfig {
            asset_id: "lisk-v2".into(),
            provider_asset_id: "lisk".into(),
            perp_symbols: vec!["LSKUSDT".into()],
            identity_evidence: "configured fixture".into(),
            chain: Some("ethereum".into()),
            contract_address: None,
        }
    }

    #[test]
    fn snapshot_requires_provider_market_cap_and_preserves_explicit_mapping() {
        let row = serde_json::json!({"current_price": 1.0, "market_cap": 20.0, "circulating_supply": 20.0, "last_updated":"2026-01-01T00:00:00Z"});
        let snapshot = snapshot_from_provider(&asset(), Some(&row), 100);
        assert_eq!(snapshot.status, "reference_available");
        assert_eq!(snapshot.perp_symbols, vec!["LSKUSDT"]);
        assert_eq!(snapshot.circulating_market_cap_usd, Some(20.0));
    }

    #[tokio::test]
    async fn store_refuses_ticker_prefix_guessing() {
        let store = SupplySnapshotStore::default();
        store
            .replace_all(vec![snapshot_from_provider(&asset(), None, 100)])
            .await;
        assert!(store.for_perp_symbol("LSKUSDT").await.is_some());
        assert!(store.for_perp_symbol("LSKUSD").await.is_none());
    }

    #[tokio::test]
    async fn disabled_service_never_attempts_a_provider_request() {
        let store = SupplySnapshotStore::default();
        let service =
            SupplyReferenceService::new(SupplyConfig::default(), store, reqwest::Client::new());
        assert!(
            service
                .ensure_for_perp_symbol("LSKUSDT")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn fresh_configured_snapshot_is_reused_without_network() {
        let store = SupplySnapshotStore::default();
        store
            .replace_all(vec![snapshot_from_provider(
                &asset(),
                Some(&serde_json::json!({"market_cap": 20.0})),
                now_ms(),
            )])
            .await;
        let service = SupplyReferenceService::new(
            SupplyConfig {
                enabled: true,
                assets: vec![asset()],
                ..SupplyConfig::default()
            },
            store,
            reqwest::Client::new(),
        );
        assert_eq!(
            service
                .ensure_for_perp_symbol("LSKUSDT")
                .await
                .unwrap()
                .unwrap()
                .asset_id,
            "lisk-v2"
        );
    }
}
