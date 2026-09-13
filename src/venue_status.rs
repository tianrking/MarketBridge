//! Auditable venue asset-operation status evidence.
//!
//! No exchange credentials are stored here. A public-announcement collector or
//! an isolated read-only account observer may submit a normalized observation,
//! but its scope is retained so account-local data cannot be represented as a
//! global market fact.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::types::now_ms;

const MAX_HISTORY: usize = 10_000;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetOperation {
    Deposit,
    Withdrawal,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Enabled,
    Disabled,
    Maintenance,
    Unknown,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationScope {
    PublicAnnouncementObserved,
    AccountObserved,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VenueAssetStatusObservation {
    pub venue: String,
    /// Stable asset identity; this must match an explicitly configured supply
    /// mapping before the squeeze radar can associate it with an OI row.
    pub asset_id: String,
    pub operation: AssetOperation,
    pub status: OperationStatus,
    pub scope: ObservationScope,
    pub observed_at_ms: u64,
    pub source_kind: String,
    pub source_url: String,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub published_at_ms: Option<u64>,
    #[serde(default)]
    pub note: Option<String>,
}

impl VenueAssetStatusObservation {
    pub fn validate(&self, now: u64) -> Result<()> {
        for text in [
            &self.venue,
            &self.asset_id,
            &self.source_kind,
            &self.source_url,
        ] {
            ensure!(
                !text.trim().is_empty() && text.len() <= 512,
                "venue status requires nonempty bounded identity and source fields"
            );
        }
        ensure!(
            self.observed_at_ms > 0 && self.observed_at_ms <= now.saturating_add(120_000),
            "venue status observed_at_ms is invalid or implausibly future dated"
        );
        ensure!(
            self.published_at_ms
                .is_none_or(|value| value > 0 && value <= self.observed_at_ms),
            "venue status published_at_ms must precede observation"
        );
        let url = reqwest::Url::parse(&self.source_url)?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none(),
            "venue status source_url must be a public credential-free HTTP(S) URL"
        );
        if self.scope == ObservationScope::AccountObserved {
            ensure!(
                self.source_kind
                    .eq_ignore_ascii_case("read_only_account_api"),
                "account-observed status must explicitly declare source_kind=read_only_account_api"
            );
        }
        for text in [&self.network, &self.note].into_iter().flatten() {
            ensure!(text.len() <= 2048, "venue status optional field too long");
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct VenueAssetStatusStore(Arc<RwLock<VenueStatusInner>>);

#[derive(Default)]
struct VenueStatusInner {
    latest: HashMap<String, VenueAssetStatusObservation>,
    history: VecDeque<VenueAssetStatusObservation>,
}

impl VenueAssetStatusStore {
    pub async fn insert(&self, observation: VenueAssetStatusObservation) -> Result<()> {
        observation.validate(now_ms())?;
        let key = status_key(&observation);
        let mut inner = self.0.write().await;
        if inner
            .latest
            .get(&key)
            .is_some_and(|old| old.observed_at_ms > observation.observed_at_ms)
        {
            return Ok(());
        }
        inner.latest.insert(key, observation.clone());
        inner.history.push_back(observation);
        while inner.history.len() > MAX_HISTORY {
            inner.history.pop_front();
        }
        Ok(())
    }

    pub async fn query(
        &self,
        venue: Option<&str>,
        asset_id: Option<&str>,
    ) -> Vec<VenueAssetStatusObservation> {
        let mut rows = self
            .0
            .read()
            .await
            .latest
            .values()
            .filter(|row| venue.is_none_or(|wanted| row.venue.eq_ignore_ascii_case(wanted)))
            .filter(|row| asset_id.is_none_or(|wanted| row.asset_id == wanted))
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            a.venue
                .cmp(&b.venue)
                .then(a.asset_id.cmp(&b.asset_id))
                .then((a.operation as u8).cmp(&(b.operation as u8)))
        });
        rows
    }
}

fn status_key(value: &VenueAssetStatusObservation) -> String {
    format!(
        "{}:{}:{:?}:{}",
        value.venue.to_ascii_lowercase(),
        value.asset_id,
        value.operation,
        value.network.as_deref().unwrap_or("*").to_ascii_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> VenueAssetStatusObservation {
        VenueAssetStatusObservation {
            venue: "binance".into(),
            asset_id: "lisk-v2".into(),
            operation: AssetOperation::Deposit,
            status: OperationStatus::Disabled,
            scope: ObservationScope::PublicAnnouncementObserved,
            observed_at_ms: now_ms(),
            source_kind: "venue_announcement".into(),
            source_url: "https://www.binance.com/en/support/announcement/example".into(),
            network: Some("ethereum".into()),
            published_at_ms: None,
            note: None,
        }
    }

    #[tokio::test]
    async fn latest_status_keeps_newer_observation() {
        let store = VenueAssetStatusStore::default();
        let first = observation();
        store.insert(first.clone()).await.unwrap();
        let mut old = first;
        old.observed_at_ms = 1;
        old.status = OperationStatus::Enabled;
        store.insert(old).await.unwrap();
        assert_eq!(
            store.query(Some("binance"), Some("lisk-v2")).await[0].status,
            OperationStatus::Disabled
        );
    }

    #[test]
    fn account_scope_requires_explicit_read_only_source_kind() {
        let mut value = observation();
        value.scope = ObservationScope::AccountObserved;
        assert!(value.validate(now_ms()).is_err());
        value.source_kind = "read_only_account_api".into();
        assert!(value.validate(now_ms()).is_ok());
    }
}
