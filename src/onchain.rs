use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::config::{EtherscanConfig, MempoolSpaceConfig, OnchainConfig, WhaleAlertConfig};

const MAX_TRANSFERS: usize = 5_000;
const ETHERSCAN_MAX_ATTEMPTS: usize = 5;
const ETHERSCAN_INITIAL_BACKOFF_MS: u64 = 500;
const ETHERSCAN_MAX_BACKOFF_MS: u64 = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct OnchainTransfer {
    pub source: String,
    pub chain: String,
    pub tx_hash: String,
    pub from_address: Option<String>,
    pub to_address: Option<String>,
    pub asset: Option<String>,
    pub amount: Option<f64>,
    pub amount_usd: Option<f64>,
    pub direction: Option<String>,
    pub block_height: Option<u64>,
    pub ts_ms: u64,
    pub url: Option<String>,
    pub raw: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct OnchainTransferQuery {
    pub source: Option<String>,
    pub chain: Option<String>,
    pub asset: Option<String>,
    pub min_amount_usd: Option<f64>,
    pub limit: usize,
}

#[derive(Clone, Default)]
pub struct OnchainTransferStore {
    inner: Arc<RwLock<OnchainTransferState>>,
}

#[derive(Default)]
struct OnchainTransferState {
    seen: HashSet<String>,
    rows: VecDeque<OnchainTransfer>,
}

impl OnchainTransferStore {
    pub async fn insert_many(&self, rows: Vec<OnchainTransfer>) {
        if rows.is_empty() {
            return;
        }
        let mut guard = self.inner.write().await;
        for row in rows {
            let key = format!("{}:{}:{}", row.source, row.chain, row.tx_hash);
            if !guard.seen.insert(key) {
                continue;
            }
            guard.rows.push_front(row);
        }
        while guard.rows.len() > MAX_TRANSFERS {
            if let Some(old) = guard.rows.pop_back() {
                guard
                    .seen
                    .remove(&format!("{}:{}:{}", old.source, old.chain, old.tx_hash));
            }
        }
    }

    pub async fn query(&self, q: OnchainTransferQuery) -> Vec<OnchainTransfer> {
        let guard = self.inner.read().await;
        guard
            .rows
            .iter()
            .filter(|row| {
                q.source
                    .as_ref()
                    .is_none_or(|value| row.source.eq_ignore_ascii_case(value))
            })
            .filter(|row| {
                q.chain
                    .as_ref()
                    .is_none_or(|value| row.chain.eq_ignore_ascii_case(value))
            })
            .filter(|row| {
                q.asset.as_ref().is_none_or(|value| {
                    row.asset
                        .as_deref()
                        .is_some_and(|asset| asset.eq_ignore_ascii_case(value))
                })
            })
            .filter(|row| {
                q.min_amount_usd
                    .is_none_or(|min| row.amount_usd.is_some_and(|amount| amount >= min))
            })
            .take(q.limit.clamp(1, 5000))
            .cloned()
            .collect()
    }
}

pub fn spawn_onchain_collectors(
    cfg: OnchainConfig,
    http: reqwest::Client,
    store: OnchainTransferStore,
    shutdown: CancellationToken,
) -> Vec<JoinHandle<()>> {
    let mut tasks = Vec::new();
    if cfg.whale_alert.enabled {
        tasks.push(spawn_whale_alert(
            cfg.whale_alert,
            http.clone(),
            store.clone(),
            shutdown.clone(),
        ));
    }
    if cfg.mempool_space.enabled {
        tasks.push(spawn_mempool_space(
            cfg.mempool_space,
            http.clone(),
            store.clone(),
            shutdown.clone(),
        ));
    }
    if cfg.etherscan.enabled {
        tasks.push(spawn_etherscan(cfg.etherscan, http, store, shutdown));
    }
    tasks
}

fn spawn_whale_alert(
    cfg: WhaleAlertConfig,
    http: reqwest::Client,
    store: OnchainTransferStore,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(api_key) = configured_key(cfg.api_key.as_deref(), &cfg.api_key_env) else {
            warn!("whale alert enabled but api key is missing");
            return;
        };
        let mut last_start = unix_secs().saturating_sub(300);
        let mut tick = tokio::time::interval(Duration::from_secs(cfg.poll_secs));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tick.tick() => {
                    match fetch_whale_alert(&http, &cfg, &api_key, last_start).await {
                        Ok(rows) => {
                            last_start = unix_secs().saturating_sub(30);
                            store.insert_many(rows).await;
                        }
                        Err(error) => warn!(%error, "whale alert poll failed"),
                    }
                }
            }
        }
    })
}

async fn fetch_whale_alert(
    http: &reqwest::Client,
    cfg: &WhaleAlertConfig,
    api_key: &str,
    start_secs: u64,
) -> Result<Vec<OnchainTransfer>> {
    let url = format!("{}transactions", cfg.base_url.trim_end_matches('/'));
    let payload = http
        .get(url)
        .query(&[
            ("api_key", api_key.to_string()),
            ("start", start_secs.to_string()),
            ("min_value", cfg.min_value_usd.to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let txs = payload
        .get("transactions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(txs
        .into_iter()
        .filter_map(|tx| {
            let hash = tx.get("hash").and_then(Value::as_str)?.to_string();
            Some(OnchainTransfer {
                source: "whale_alert".to_string(),
                chain: tx
                    .get("blockchain")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                tx_hash: hash.clone(),
                from_address: tx
                    .get("from")
                    .and_then(|v| v.get("address"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                to_address: tx
                    .get("to")
                    .and_then(|v| v.get("address"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                asset: tx
                    .get("symbol")
                    .and_then(Value::as_str)
                    .map(|x| x.to_ascii_uppercase()),
                amount: tx.get("amount").and_then(Value::as_f64),
                amount_usd: tx.get("amount_usd").and_then(Value::as_f64),
                direction: tx
                    .get("transaction_type")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                block_height: tx.get("block").and_then(Value::as_u64),
                ts_ms: tx.get("timestamp").and_then(Value::as_u64).unwrap_or(0) * 1000,
                url: Some(format!("https://whale-alert.io/transaction/{hash}")),
                raw: Some(tx),
            })
        })
        .collect())
}

fn spawn_mempool_space(
    cfg: MempoolSpaceConfig,
    http: reqwest::Client,
    store: OnchainTransferStore,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(cfg.poll_secs));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tick.tick() => {
                    match fetch_mempool_space(&http, &cfg).await {
                        Ok(rows) => store.insert_many(rows).await,
                        Err(error) => warn!(%error, "mempool.space poll failed"),
                    }
                }
            }
        }
    })
}

async fn fetch_mempool_space(
    http: &reqwest::Client,
    cfg: &MempoolSpaceConfig,
) -> Result<Vec<OnchainTransfer>> {
    let url = format!("{}mempool/recent", cfg.base_url.trim_end_matches('/'));
    let rows = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await
        .context("failed to parse mempool.space recent payload")?;
    let min_sats = cfg.min_value_btc * 100_000_000.0;
    Ok(rows
        .into_iter()
        .filter_map(|tx| {
            let value_sats = tx
                .get("value")
                .and_then(Value::as_f64)
                .or_else(|| tx.get("vout").and_then(sum_vout_sats))?;
            if value_sats < min_sats {
                return None;
            }
            let txid = tx.get("txid").and_then(Value::as_str)?.to_string();
            Some(OnchainTransfer {
                source: "mempool_space".to_string(),
                chain: "bitcoin".to_string(),
                tx_hash: txid.clone(),
                from_address: None,
                to_address: None,
                asset: Some("BTC".to_string()),
                amount: Some(value_sats / 100_000_000.0),
                amount_usd: None,
                direction: Some("mempool_large_tx".to_string()),
                block_height: None,
                ts_ms: crate::types::now_ms(),
                url: Some(format!("https://mempool.space/tx/{txid}")),
                raw: Some(tx),
            })
        })
        .collect())
}

fn spawn_etherscan(
    cfg: EtherscanConfig,
    http: reqwest::Client,
    store: OnchainTransferStore,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(api_key) = configured_key(cfg.api_key.as_deref(), &cfg.api_key_env) else {
            warn!("etherscan enabled but api key is missing");
            return;
        };
        if cfg.addresses.is_empty() {
            warn!("etherscan enabled but no watch addresses configured");
            return;
        }
        let mut tick = tokio::time::interval(Duration::from_secs(cfg.poll_secs));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tick.tick() => {
                    let safe_to_block = match fetch_etherscan_safe_to_block(&http, &cfg, &api_key).await {
                        Ok(block) => block,
                        Err(error) => {
                            warn!(%error, "etherscan latest block poll failed");
                            continue;
                        }
                    };
                    for address in &cfg.addresses {
                        match fetch_etherscan_address(&http, &cfg, &api_key, address, safe_to_block).await {
                            Ok(rows) => store.insert_many(rows).await,
                            Err(error) => warn!(%error, address, "etherscan poll failed"),
                        }
                        tokio::select! {
                            _ = shutdown.cancelled() => break,
                            _ = tokio::time::sleep(Duration::from_millis(cfg.request_delay_ms)) => {}
                        }
                    }
                }
            }
        }
    })
}

async fn fetch_etherscan_address(
    http: &reqwest::Client,
    cfg: &EtherscanConfig,
    api_key: &str,
    address: &str,
    safe_to_block: u64,
) -> Result<Vec<OnchainTransfer>> {
    let payload = get_etherscan_json(
        http,
        cfg,
        &[
            ("module", "account".to_string()),
            ("action", "txlist".to_string()),
            ("address", address.to_string()),
            ("sort", "desc".to_string()),
            ("page", "1".to_string()),
            ("offset", "100".to_string()),
            ("endblock", safe_to_block.to_string()),
            ("apikey", api_key.to_string()),
        ],
    )
    .await?;
    let rows = payload
        .get("result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(rows
        .into_iter()
        .filter_map(|tx| {
            let wei = tx
                .get("value")
                .and_then(Value::as_str)
                .and_then(|x| x.parse::<f64>().ok())?;
            let eth = wei / 1e18;
            if eth < cfg.min_value_eth {
                return None;
            }
            Some(OnchainTransfer {
                source: "etherscan".to_string(),
                chain: "ethereum".to_string(),
                tx_hash: tx.get("hash").and_then(Value::as_str)?.to_string(),
                from_address: tx.get("from").and_then(Value::as_str).map(str::to_string),
                to_address: tx.get("to").and_then(Value::as_str).map(str::to_string),
                asset: Some("ETH".to_string()),
                amount: Some(eth),
                amount_usd: None,
                direction: Some(
                    if tx
                        .get("from")
                        .and_then(Value::as_str)
                        .is_some_and(|from| from.eq_ignore_ascii_case(address))
                    {
                        "outbound"
                    } else {
                        "inbound"
                    }
                    .to_string(),
                ),
                block_height: tx
                    .get("blockNumber")
                    .and_then(Value::as_str)
                    .and_then(|x| x.parse().ok()),
                ts_ms: tx
                    .get("timeStamp")
                    .and_then(Value::as_str)
                    .and_then(|x| x.parse::<u64>().ok())
                    .unwrap_or(0)
                    * 1000,
                url: tx
                    .get("hash")
                    .and_then(Value::as_str)
                    .map(|hash| format!("https://etherscan.io/tx/{hash}")),
                raw: Some(tx),
            })
        })
        .collect())
}

async fn fetch_etherscan_safe_to_block(
    http: &reqwest::Client,
    cfg: &EtherscanConfig,
    api_key: &str,
) -> Result<u64> {
    let payload = get_etherscan_json(
        http,
        cfg,
        &[
            ("module", "proxy".to_string()),
            ("action", "eth_blockNumber".to_string()),
            ("apikey", api_key.to_string()),
        ],
    )
    .await?;
    let latest = payload
        .get("result")
        .and_then(Value::as_str)
        .and_then(|hex| u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok())
        .context("etherscan eth_blockNumber result missing")?;
    Ok(latest.saturating_sub(cfg.safe_confirmations))
}

async fn get_etherscan_json(
    http: &reqwest::Client,
    cfg: &EtherscanConfig,
    query: &[(&str, String)],
) -> Result<Value> {
    let mut backoff = Duration::from_millis(ETHERSCAN_INITIAL_BACKOFF_MS);
    for attempt in 1..=ETHERSCAN_MAX_ATTEMPTS {
        let response = http.get(&cfg.base_url).query(query).send().await;
        match response {
            Ok(response)
                if response.status() == StatusCode::TOO_MANY_REQUESTS
                    || response.status().is_server_error() =>
            {
                warn!(
                    status = response.status().as_u16(),
                    attempt, "etherscan request rate-limited or transiently failed"
                );
            }
            Ok(response) => {
                let payload = response.error_for_status()?.json::<Value>().await?;
                if etherscan_payload_is_rate_limited(&payload) {
                    warn!(attempt, "etherscan payload reports rate limit");
                } else {
                    return Ok(payload);
                }
            }
            Err(error) => {
                warn!(%error, attempt, "etherscan request failed");
            }
        }

        if attempt < ETHERSCAN_MAX_ATTEMPTS {
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_millis(ETHERSCAN_MAX_BACKOFF_MS));
        }
    }
    anyhow::bail!("etherscan request failed after {ETHERSCAN_MAX_ATTEMPTS} attempts")
}

fn etherscan_payload_is_rate_limited(payload: &Value) -> bool {
    let message = ["status", "message", "result"]
        .into_iter()
        .filter_map(|key| payload.get(key).and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    message.contains("rate limit") || message.contains("max rate") || message.contains("too many")
}

fn sum_vout_sats(value: &Value) -> Option<f64> {
    Some(
        value
            .as_array()?
            .iter()
            .filter_map(|item| item.get("value").and_then(Value::as_f64))
            .sum(),
    )
}

fn configured_key(inline: Option<&str>, env_name: &str) -> Option<String> {
    inline
        .filter(|key| !key.trim().is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var(env_name).ok())
        .filter(|key| !key.trim().is_empty())
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn log_onchain_start(cfg: &OnchainConfig) {
    if cfg.whale_alert.enabled || cfg.mempool_space.enabled || cfg.etherscan.enabled {
        info!("onchain collectors enabled");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OnchainTransfer, OnchainTransferQuery, OnchainTransferStore,
        etherscan_payload_is_rate_limited,
    };
    use serde_json::json;

    #[tokio::test]
    async fn onchain_store_deduplicates_and_filters() {
        let store = OnchainTransferStore::default();
        store
            .insert_many(vec![
                OnchainTransfer {
                    source: "whale_alert".to_string(),
                    chain: "bitcoin".to_string(),
                    tx_hash: "abc".to_string(),
                    from_address: None,
                    to_address: None,
                    asset: Some("BTC".to_string()),
                    amount: Some(100.0),
                    amount_usd: Some(1_000_000.0),
                    direction: None,
                    block_height: None,
                    ts_ms: 1,
                    url: None,
                    raw: None,
                },
                OnchainTransfer {
                    source: "whale_alert".to_string(),
                    chain: "bitcoin".to_string(),
                    tx_hash: "abc".to_string(),
                    from_address: None,
                    to_address: None,
                    asset: Some("BTC".to_string()),
                    amount: Some(100.0),
                    amount_usd: Some(1_000_000.0),
                    direction: None,
                    block_height: None,
                    ts_ms: 1,
                    url: None,
                    raw: None,
                },
            ])
            .await;

        let rows = store
            .query(OnchainTransferQuery {
                source: Some("whale_alert".to_string()),
                chain: Some("bitcoin".to_string()),
                asset: Some("BTC".to_string()),
                min_amount_usd: Some(500_000.0),
                limit: 10,
            })
            .await;
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn etherscan_rate_limit_payloads_are_detected() {
        assert!(etherscan_payload_is_rate_limited(&json!({
            "status": "0",
            "message": "NOTOK",
            "result": "Max rate limit reached"
        })));
        assert!(!etherscan_payload_is_rate_limited(&json!({
            "status": "1",
            "message": "OK",
            "result": []
        })));
    }
}
