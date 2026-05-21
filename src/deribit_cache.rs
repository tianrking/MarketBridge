use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};
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

    pub async fn upsert_live_summary(&self, source: &str, summary: OptionSummary) {
        let received_at_ms = now_ms();
        let mut guard = self.inner.write().await;
        if let Some(row) = guard.iter_mut().find(|row| {
            row.summary
                .venue
                .eq_ignore_ascii_case(summary.venue.as_str())
                && row.summary.instrument_name == summary.instrument_name
        }) {
            row.source = source.to_string();
            row.received_at_ms = received_at_ms;
            row.stale = false;
            row.summary = merge_option_summary(row.summary.clone(), summary);
            return;
        }

        guard.push(CachedOptionSummary {
            version: "v1",
            source: source.to_string(),
            received_at_ms,
            stale: false,
            summary,
        });
    }

    pub async fn filtered(&self, filter: OptionFilter) -> Vec<CachedOptionSummary> {
        let venue = filter.venue.map(|x| x.trim().to_ascii_lowercase());
        let currency = filter.currency.map(|x| x.trim().to_ascii_uppercase());
        let option_type = filter.option_type.map(|x| x.trim().to_ascii_lowercase());
        let guard = self.inner.read().await;
        let now_ms = now_ms();
        guard
            .iter()
            .filter(|row| {
                filter.include_stale
                    || now_ms.saturating_sub(row.received_at_ms) <= self.stale_ttl_ms
            })
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
            .cloned()
            .map(|mut row| {
                row.stale = now_ms.saturating_sub(row.received_at_ms) > self.stale_ttl_ms;
                row
            })
            .collect()
    }
}

pub type DeribitOptionFilter = OptionFilter;
pub type DeribitOptionCache = OptionCache;

macro_rules! option_cache_spawner {
    ($name:ident, $cfg:ty, $venue:literal, $fetch:path) => {
        pub fn $name(
            cfg: $cfg,
            client: reqwest::Client,
            cache: OptionCache,
            shutdown: CancellationToken,
        ) -> tokio::task::JoinHandle<()> {
            spawn_option_cache(
                OptionCacheJob {
                    venue: $venue,
                    base_url: cfg.base_url,
                    currencies: cfg.currencies,
                    refresh_secs: cfg.refresh_secs,
                    client,
                    cache,
                    shutdown,
                },
                |client, base_url, currency| async move {
                    $fetch(&client, &base_url, &currency).await
                },
            )
        }
    };
}

option_cache_spawner!(
    spawn_deribit_option_cache,
    DeribitConfig,
    "deribit",
    fetch_deribit_option_summaries_from
);
option_cache_spawner!(
    spawn_okx_option_cache,
    OkxOptionsConfig,
    "okx",
    fetch_okx_option_summaries_from
);
option_cache_spawner!(
    spawn_bybit_option_cache,
    BybitOptionsConfig,
    "bybit",
    fetch_bybit_option_summaries_from
);
option_cache_spawner!(
    spawn_binance_option_cache,
    BinanceOptionsConfig,
    "binance",
    fetch_binance_option_summaries_from
);

pub fn spawn_deribit_option_ws_cache(
    cfg: DeribitConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !cfg.enabled {
            return;
        }
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            match run_deribit_option_ws_once(&cfg, client.clone(), cache.clone(), shutdown.clone())
                .await
            {
                Ok(()) => {}
                Err(error) => warn!(%error, "deribit option websocket stopped"),
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    })
}

pub fn spawn_bybit_option_ws_cache(
    cfg: BybitOptionsConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !cfg.enabled {
            return;
        }
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            match run_bybit_option_ws_once(&cfg, client.clone(), cache.clone(), shutdown.clone())
                .await
            {
                Ok(()) => {}
                Err(error) => warn!(%error, "bybit option websocket stopped"),
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    })
}

async fn run_bybit_option_ws_once(
    cfg: &BybitOptionsConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> Result<()> {
    let currencies = normalized_currencies(&cfg.currencies);
    if currencies.is_empty() {
        return Ok(());
    }

    let mut instruments = Vec::new();
    for currency in &currencies {
        let rows = fetch_bybit_option_summaries_from(&client, &cfg.base_url, currency).await?;
        instruments.extend(rows.into_iter().map(|row| row.instrument_name));
    }
    instruments.sort();
    instruments.dedup();
    if instruments.is_empty() {
        return Ok(());
    }

    let args = instruments
        .iter()
        .map(|instrument| format!("tickers.{instrument}"))
        .collect::<Vec<_>>();

    let (ws, _) = connect_async(&cfg.ws_url)
        .await
        .context("bybit option websocket connect failed")?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(
        json!({
            "op": "subscribe",
            "args": args
        })
        .to_string(),
    ))
    .await?;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            msg = stream.next() => {
                let msg = msg.context("bybit option websocket ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Some(summary) = parse_bybit_ws_option_summary(&text) {
                            cache.upsert_live_summary("bybit_ws_ticker", summary).await;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                    Message::Close(_) => return Ok(()),
                }
            }
        }
    }
}

async fn run_deribit_option_ws_once(
    cfg: &DeribitConfig,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
) -> Result<()> {
    let currencies = normalized_currencies(&cfg.currencies);
    if currencies.is_empty() {
        return Ok(());
    }

    let mut instruments = Vec::new();
    for currency in &currencies {
        let rows = fetch_deribit_option_summaries_from(&client, &cfg.base_url, currency).await?;
        instruments.extend(rows.into_iter().map(|row| row.instrument_name));
    }
    instruments.sort();
    instruments.dedup();
    if instruments.is_empty() {
        return Ok(());
    }

    let channels = instruments
        .iter()
        .map(|instrument| format!("ticker.{instrument}.100ms"))
        .collect::<Vec<_>>();

    let (ws, _) = connect_async(&cfg.ws_url)
        .await
        .context("deribit option websocket connect failed")?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "public/subscribe",
            "params": {"channels": channels}
        })
        .to_string(),
    ))
    .await?;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            msg = stream.next() => {
                let msg = msg.context("deribit option websocket ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Some(summary) = parse_deribit_ws_option_summary(&text) {
                            cache.upsert_live_summary("deribit_ws_ticker", summary).await;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                    Message::Close(_) => return Ok(()),
                }
            }
        }
    }
}

fn parse_bybit_ws_option_summary(text: &str) -> Option<OptionSummary> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let data = value.get("data")?;
    let row = data
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(data);
    let symbol = row
        .get("symbol")
        .or_else(|| row.get("s"))
        .and_then(Value::as_str)?;
    let currency = symbol.split('-').next()?.to_ascii_uppercase();
    let parsed = parse_bybit_option_instrument(symbol);
    Some(OptionSummary {
        venue: "bybit".to_string(),
        currency,
        instrument_name: symbol.to_string(),
        option_type: parsed.as_ref().map(|p| p.2.clone()),
        strike: parsed.as_ref().map(|p| p.1),
        expiry_time: parsed.map(|p| p.0),
        bid_price: json_f64(row, &["bid1Price", "bidPrice", "bp"]),
        ask_price: json_f64(row, &["ask1Price", "askPrice", "ap"]),
        mark_price: json_f64(row, &["markPrice", "mp"]),
        mark_iv: json_f64(row, &["markIv", "markIV", "iv"]),
        delta: json_f64(row, &["delta"]),
        gamma: json_f64(row, &["gamma"]),
        theta: json_f64(row, &["theta"]),
        vega: json_f64(row, &["vega"]),
        underlying_price: json_f64(row, &["underlyingPrice", "indexPrice"]),
        underlying_index: Some(symbol.split('-').next()?.to_ascii_uppercase()),
        open_interest: json_f64(row, &["openInterest", "oi"]),
    })
}

fn parse_deribit_ws_option_summary(text: &str) -> Option<OptionSummary> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let row = value.pointer("/params/data")?;
    let instrument_name = row.get("instrument_name").and_then(Value::as_str)?;
    let currency = instrument_name.split('-').next()?.to_ascii_uppercase();
    let parsed = parse_deribit_option_instrument(instrument_name);
    let greeks = row.get("greeks");
    Some(OptionSummary {
        venue: "deribit".to_string(),
        currency,
        instrument_name: instrument_name.to_string(),
        option_type: parsed.as_ref().map(|p| p.2.clone()),
        strike: parsed.as_ref().map(|p| p.1),
        expiry_time: parsed.map(|p| p.0),
        bid_price: row.get("best_bid_price").and_then(Value::as_f64),
        ask_price: row.get("best_ask_price").and_then(Value::as_f64),
        mark_price: row.get("mark_price").and_then(Value::as_f64),
        mark_iv: row.get("mark_iv").and_then(Value::as_f64),
        delta: greeks.and_then(|g| g.get("delta")).and_then(Value::as_f64),
        gamma: greeks.and_then(|g| g.get("gamma")).and_then(Value::as_f64),
        theta: greeks.and_then(|g| g.get("theta")).and_then(Value::as_f64),
        vega: greeks.and_then(|g| g.get("vega")).and_then(Value::as_f64),
        underlying_price: row.get("underlying_price").and_then(Value::as_f64),
        underlying_index: row
            .get("underlying_index")
            .and_then(Value::as_str)
            .map(str::to_string),
        open_interest: row.get("open_interest").and_then(Value::as_f64),
    })
}

fn parse_bybit_option_instrument(name: &str) -> Option<(String, f64, String)> {
    let parts = name.split('-').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    let expiry_time = crate::connectors::options::common::parse_day_month_year_expiry(parts[1])?;
    let strike = parts[2].parse::<f64>().ok()?;
    let option_type = crate::connectors::options::common::option_side_from_code(parts[3]);
    Some((expiry_time, strike, option_type))
}

fn parse_deribit_option_instrument(name: &str) -> Option<(String, f64, String)> {
    let parts = name.split('-').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    let expiry_time = crate::connectors::options::common::parse_day_month_year_expiry(parts[1])?;
    let strike = parts[2].parse::<f64>().ok()?;
    let option_type = crate::connectors::options::common::option_side_from_code(parts[3]);
    Some((expiry_time, strike, option_type))
}

fn json_f64(row: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        row.get(*key).and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        })
    })
}

fn merge_option_summary(mut old: OptionSummary, live: OptionSummary) -> OptionSummary {
    old.bid_price = live.bid_price.or(old.bid_price);
    old.ask_price = live.ask_price.or(old.ask_price);
    old.mark_price = live.mark_price.or(old.mark_price);
    old.mark_iv = live.mark_iv.or(old.mark_iv);
    old.delta = live.delta.or(old.delta);
    old.gamma = live.gamma.or(old.gamma);
    old.theta = live.theta.or(old.theta);
    old.vega = live.vega.or(old.vega);
    old.underlying_price = live.underlying_price.or(old.underlying_price);
    old.underlying_index = live.underlying_index.or(old.underlying_index);
    old.open_interest = live.open_interest.or(old.open_interest);
    old
}

struct OptionCacheJob {
    venue: &'static str,
    base_url: String,
    currencies: Vec<String>,
    refresh_secs: u64,
    client: reqwest::Client,
    cache: OptionCache,
    shutdown: CancellationToken,
}

fn spawn_option_cache<F, Fut>(job: OptionCacheJob, fetch: F) -> tokio::task::JoinHandle<()>
where
    F: Fn(reqwest::Client, String, String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<Vec<OptionSummary>>> + Send,
{
    let OptionCacheJob {
        venue,
        base_url,
        currencies,
        refresh_secs,
        client,
        cache,
        shutdown,
    } = job;
    tokio::spawn(async move {
        let currencies = normalized_currencies(&currencies);
        if currencies.is_empty() {
            warn!(venue, "option cache enabled with empty currencies");
            return;
        }
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            for currency in &currencies {
                match fetch(client.clone(), base_url.clone(), currency.clone()).await {
                    Ok(rows) => {
                        let count = rows.len();
                        cache.replace_venue_currency(venue, currency, rows).await;
                        info!(venue, currency, count, "option cache refreshed");
                    }
                    Err(error) => {
                        warn!(venue, currency, %error, "option cache refresh failed");
                    }
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(refresh_secs.max(1))) => {}
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
            delta: None,
            gamma: None,
            theta: None,
            vega: None,
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

    #[test]
    fn parses_deribit_ws_ticker_summary() {
        let summary = parse_deribit_ws_option_summary(
            &json!({
                "jsonrpc": "2.0",
                "method": "subscription",
                "params": {
                    "channel": "ticker.BTC-29MAY26-70000-P.100ms",
                    "data": {
                        "instrument_name": "BTC-29MAY26-70000-P",
                        "best_bid_price": 0.0016,
                        "best_ask_price": 0.0019,
                        "mark_price": 0.0017,
                        "mark_iv": 46.4,
                        "underlying_price": 77967.19,
                        "underlying_index": "BTC-29MAY26",
                        "open_interest": 3544.6,
                        "greeks": {
                            "delta": -0.05645,
                            "gamma": 0.00002,
                            "theta": -37.55606,
                            "vega": 13.26425
                        }
                    }
                }
            })
            .to_string(),
        )
        .expect("summary");

        assert_eq!(summary.venue, "deribit");
        assert_eq!(summary.currency, "BTC");
        assert_eq!(summary.option_type.as_deref(), Some("put"));
        assert_eq!(summary.delta, Some(-0.05645));
        assert_eq!(summary.open_interest, Some(3544.6));
    }

    #[test]
    fn parses_bybit_ws_ticker_summary() {
        let summary = parse_bybit_ws_option_summary(
            &json!({
                "topic": "tickers.BTC-26MAR27-78000-P-USDT",
                "type": "snapshot",
                "data": {
                    "symbol": "BTC-26MAR27-78000-P-USDT",
                    "bid1Price": "10745",
                    "ask1Price": "13140",
                    "markPrice": "11900",
                    "markIv": "0.55",
                    "underlyingPrice": "77967.19",
                    "openInterest": "123.4",
                    "delta": "-0.12",
                    "gamma": "0.00002",
                    "theta": "-10.5",
                    "vega": "20.1"
                }
            })
            .to_string(),
        )
        .expect("summary");

        assert_eq!(summary.venue, "bybit");
        assert_eq!(summary.currency, "BTC");
        assert_eq!(summary.option_type.as_deref(), Some("put"));
        assert_eq!(summary.bid_price, Some(10745.0));
        assert_eq!(summary.open_interest, Some(123.4));
        assert_eq!(summary.vega, Some(20.1));
    }
}
