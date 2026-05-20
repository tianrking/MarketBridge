use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

use crate::config::PolymarketConfig;
use crate::external::{
    PolymarketBookLevel, PolymarketBookSummary, PolymarketOrderBook,
    fetch_polymarket_crypto_markets, summarize_book,
};
use crate::types::now_ms;

#[derive(Debug, Clone, Serialize)]
pub struct CachedPolymarketBook {
    pub version: &'static str,
    pub source: &'static str,
    pub market: Option<String>,
    pub asset_id: String,
    pub timestamp: Option<String>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub bid_depth: Option<f64>,
    pub ask_depth: Option<f64>,
    pub raw_bid_levels: Option<usize>,
    pub raw_ask_levels: Option<usize>,
    pub last_event_type: String,
    pub received_at_ms: u64,
    pub source_latency_ms: Option<u64>,
    pub stale: bool,
}

#[derive(Clone)]
pub struct PolymarketBookCache {
    inner: Arc<RwLock<HashMap<String, CachedPolymarketBook>>>,
    stale_ttl_ms: u64,
}

impl PolymarketBookCache {
    pub fn new(stale_ttl_ms: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            stale_ttl_ms,
        }
    }

    pub async fn upsert_rest_summary(&self, summary: PolymarketBookSummary) {
        self.upsert(CachedPolymarketBook {
            version: "v1",
            source: "polymarket_clob_rest",
            market: summary.market,
            asset_id: summary.asset_id,
            timestamp: summary.timestamp,
            best_bid: summary.best_bid,
            best_ask: summary.best_ask,
            spread: summary.spread,
            bid_depth: Some(summary.bid_depth),
            ask_depth: Some(summary.ask_depth),
            raw_bid_levels: Some(summary.raw_bid_levels),
            raw_ask_levels: Some(summary.raw_ask_levels),
            last_event_type: "book".to_string(),
            received_at_ms: now_ms(),
            source_latency_ms: None,
            stale: false,
        })
        .await;
    }

    pub async fn apply_ws_payload(&self, payload: &str) -> Result<usize> {
        let value: Value =
            serde_json::from_str(payload).unwrap_or_else(|_| Value::String(payload.into()));
        match value {
            Value::Array(items) => {
                let mut count = 0usize;
                for item in items {
                    count += self.apply_ws_value(&item).await?;
                }
                Ok(count)
            }
            item => self.apply_ws_value(&item).await,
        }
    }

    pub async fn all(&self) -> Vec<CachedPolymarketBook> {
        let guard = self.inner.read().await;
        guard
            .values()
            .cloned()
            .map(|row| self.with_stale(row))
            .collect()
    }

    pub async fn by_ids(&self, token_ids: &[String]) -> Vec<CachedPolymarketBook> {
        let guard = self.inner.read().await;
        token_ids
            .iter()
            .filter_map(|id| guard.get(id).cloned())
            .map(|row| self.with_stale(row))
            .collect()
    }

    async fn apply_ws_value(&self, value: &Value) -> Result<usize> {
        let event_type = string_field(value, "event_type")
            .or_else(|| string_field(value, "type"))
            .unwrap_or_else(|| "unknown".to_string());
        match event_type.as_str() {
            "book" => {
                let Some(asset_id) = string_field(value, "asset_id") else {
                    return Ok(0);
                };
                let book = PolymarketOrderBook {
                    market: string_field(value, "market"),
                    asset_id,
                    timestamp: string_field(value, "timestamp"),
                    hash: string_field(value, "hash"),
                    bids: levels(value.get("bids").and_then(Value::as_array)),
                    asks: levels(value.get("asks").and_then(Value::as_array)),
                };
                let summary = summarize_book(book);
                self.upsert_rest_summary_with_source(summary, "polymarket_clob_ws", "book")
                    .await;
                Ok(1)
            }
            "best_bid_ask" => {
                let Some(asset_id) = string_field(value, "asset_id") else {
                    return Ok(0);
                };
                self.upsert_patch(
                    asset_id,
                    string_field(value, "market"),
                    string_field(value, "timestamp"),
                    parse_f64_field(value, "best_bid"),
                    parse_f64_field(value, "best_ask"),
                    "best_bid_ask",
                    signed_latency(value),
                )
                .await;
                Ok(1)
            }
            "price_change" => {
                let Some(changes) = value.get("price_changes").and_then(Value::as_array) else {
                    return Ok(0);
                };
                let mut count = 0usize;
                for change in changes {
                    let Some(asset_id) = string_field(change, "asset_id")
                        .or_else(|| string_field(change, "asset"))
                        .or_else(|| string_field(change, "token_id"))
                    else {
                        continue;
                    };
                    self.upsert_patch(
                        asset_id,
                        string_field(value, "market"),
                        string_field(value, "timestamp"),
                        parse_f64_field(change, "best_bid"),
                        parse_f64_field(change, "best_ask"),
                        "price_change",
                        signed_latency(value),
                    )
                    .await;
                    count += 1;
                }
                Ok(count)
            }
            _ => Ok(0),
        }
    }

    async fn upsert_rest_summary_with_source(
        &self,
        summary: PolymarketBookSummary,
        source: &'static str,
        event_type: &str,
    ) {
        self.upsert(CachedPolymarketBook {
            version: "v1",
            source,
            market: summary.market,
            asset_id: summary.asset_id,
            timestamp: summary.timestamp,
            best_bid: summary.best_bid,
            best_ask: summary.best_ask,
            spread: summary.spread,
            bid_depth: Some(summary.bid_depth),
            ask_depth: Some(summary.ask_depth),
            raw_bid_levels: Some(summary.raw_bid_levels),
            raw_ask_levels: Some(summary.raw_ask_levels),
            last_event_type: event_type.to_string(),
            received_at_ms: now_ms(),
            source_latency_ms: None,
            stale: false,
        })
        .await;
    }

    async fn upsert_patch(
        &self,
        asset_id: String,
        market: Option<String>,
        timestamp: Option<String>,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
        event_type: &str,
        source_latency_ms: Option<u64>,
    ) {
        let received_at_ms = now_ms();
        let mut guard = self.inner.write().await;
        let existing = guard.get(&asset_id).cloned();
        let best_bid = best_bid.or_else(|| existing.as_ref().and_then(|row| row.best_bid));
        let best_ask = best_ask.or_else(|| existing.as_ref().and_then(|row| row.best_ask));
        guard.insert(
            asset_id.clone(),
            CachedPolymarketBook {
                version: "v1",
                source: "polymarket_clob_ws",
                market: market.or_else(|| existing.as_ref().and_then(|row| row.market.clone())),
                asset_id,
                timestamp: timestamp
                    .or_else(|| existing.as_ref().and_then(|row| row.timestamp.clone())),
                best_bid,
                best_ask,
                spread: best_bid.zip(best_ask).map(|(bid, ask)| ask - bid),
                bid_depth: existing.as_ref().and_then(|row| row.bid_depth),
                ask_depth: existing.as_ref().and_then(|row| row.ask_depth),
                raw_bid_levels: existing.as_ref().and_then(|row| row.raw_bid_levels),
                raw_ask_levels: existing.as_ref().and_then(|row| row.raw_ask_levels),
                last_event_type: event_type.to_string(),
                received_at_ms,
                source_latency_ms,
                stale: false,
            },
        );
    }

    async fn upsert(&self, row: CachedPolymarketBook) {
        let mut guard = self.inner.write().await;
        guard.insert(row.asset_id.clone(), row);
    }

    fn with_stale(&self, mut row: CachedPolymarketBook) -> CachedPolymarketBook {
        row.stale = now_ms().saturating_sub(row.received_at_ms) > self.stale_ttl_ms;
        row
    }
}

pub fn spawn_polymarket_ws_cache(
    cfg: PolymarketConfig,
    client: reqwest::Client,
    cache: PolymarketBookCache,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(error) = run_polymarket_ws_cache(&cfg, &client, &cache).await {
                warn!(%error, "polymarket ws cache cycle failed");
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    })
}

async fn run_polymarket_ws_cache(
    cfg: &PolymarketConfig,
    client: &reqwest::Client,
    cache: &PolymarketBookCache,
) -> Result<()> {
    if cfg.chunk_size == 0 {
        bail!("polymarket chunk_size must be > 0");
    }
    let response =
        fetch_polymarket_crypto_markets(client, &cfg.gamma_base_url, cfg.limit, cfg.max_offset)
            .await?;
    if response.clob_asset_ids.is_empty() {
        warn!("polymarket discovery returned no crypto CLOB assets");
        tokio::time::sleep(Duration::from_secs(cfg.refresh_secs.max(1))).await;
        return Ok(());
    }

    for result in crate::external::fetch_polymarket_books(client, &response.clob_asset_ids).await {
        match result {
            Ok(summary) => cache.upsert_rest_summary(summary).await,
            Err(error) => warn!(%error, "failed to seed polymarket book snapshot"),
        }
    }

    info!(
        assets = response.clob_asset_ids.len(),
        markets = response.markets.len(),
        "polymarket ws cache subscribing"
    );
    run_ws_connection(cfg, cache, &response.clob_asset_ids).await
}

async fn run_ws_connection(
    cfg: &PolymarketConfig,
    cache: &PolymarketBookCache,
    assets: &[String],
) -> Result<()> {
    let (mut socket, _) = connect_async(&cfg.ws_url)
        .await
        .with_context(|| format!("failed to connect {}", cfg.ws_url))?;
    for chunk in assets.chunks(cfg.chunk_size) {
        socket
            .send(Message::Text(
                json!({
                    "assets_ids": chunk,
                    "type": "market",
                    "custom_feature_enabled": true
                })
                .to_string()
                .into(),
            ))
            .await?;
    }

    let mut ping = tokio::time::interval(Duration::from_secs(cfg.ping_secs.max(1)));
    let mut refresh = tokio::time::interval(Duration::from_secs(cfg.refresh_secs.max(1)));
    loop {
        tokio::select! {
            _ = ping.tick() => {
                socket.send(Message::Text("PING".into())).await?;
            }
            _ = refresh.tick() => {
                return Ok(());
            }
            message = socket.next() => {
                let Some(message) = message else { return Ok(()); };
                let message = message?;
                if let Some(payload) = payload_text(message)? {
                    if let Err(error) = cache.apply_ws_payload(&payload).await {
                        warn!(%error, "failed to apply polymarket ws payload");
                    }
                }
            }
        }
    }
}

fn payload_text(message: Message) -> Result<Option<String>> {
    match message {
        Message::Text(text) => {
            let text = text.to_string();
            Ok((text != "PONG").then_some(text))
        }
        Message::Binary(bytes) => Ok(Some(String::from_utf8(bytes.to_vec())?)),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Ok(None),
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn parse_f64_field(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_str()?.parse::<f64>().ok()
}

fn levels(items: Option<&Vec<Value>>) -> Vec<PolymarketBookLevel> {
    items
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(PolymarketBookLevel {
                price: string_field(item, "price")?,
                size: string_field(item, "size")?,
            })
        })
        .collect()
}

fn signed_latency(value: &Value) -> Option<u64> {
    let ts = string_field(value, "timestamp")?.parse::<u64>().ok()?;
    Some(now_ms().saturating_sub(ts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn applies_price_change_to_cache() {
        let cache = PolymarketBookCache::new(1000);
        cache
            .apply_ws_payload(
                r#"{"event_type":"price_change","market":"m","timestamp":"1","price_changes":[{"asset_id":"a","best_bid":"0.40","best_ask":"0.42"}]}"#,
            )
            .await
            .unwrap();
        let rows = cache.by_ids(&["a".to_string()]).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].best_bid, Some(0.40));
        assert_eq!(rows[0].best_ask, Some(0.42));
        assert!((rows[0].spread.unwrap() - 0.02).abs() < 1e-9);
    }
}
