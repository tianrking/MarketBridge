use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::config::{AppConfig, KlineConfig};
use crate::event_bus::EventBus;
use crate::event_snapshots::NormalizedTick;

const WRITER_BATCH_MAX: usize = 256;
const WRITER_FLUSH_MS: u64 = 1_000;
const REALTIME_BAR_RETENTION_WINDOWS: u64 = 1_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct KlineBar {
    pub exchange: String,
    pub market: String,
    pub symbol: String,
    pub interval: String,
    pub open_time_ms: u64,
    pub close_time_ms: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub source: String,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct KlineQuery {
    pub exchange: Option<String>,
    pub market: Option<String>,
    pub symbol: Option<String>,
    pub interval: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub limit: usize,
}

#[derive(Clone)]
pub struct KlineStore {
    path: String,
    writer: Arc<Mutex<Option<std_mpsc::Sender<KlineWriteRequest>>>>,
}

struct KlineWriteRequest {
    rows: Vec<KlineBar>,
    respond_to: oneshot::Sender<Result<()>>,
}

impl KlineStore {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            writer: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn init(&self) -> Result<()> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || init_db(&path)).await??;

        let mut writer = self.writer.lock().expect("kline writer mutex poisoned");
        if writer.is_some() {
            return Ok(());
        }
        let path = self.path.clone();
        let (tx, rx) = std_mpsc::channel::<KlineWriteRequest>();
        tokio::task::spawn_blocking(move || run_sqlite_writer(path, rx));
        *writer = Some(tx);
        Ok(())
    }

    pub async fn upsert_many(&self, rows: Vec<KlineBar>) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let tx = self
            .writer
            .lock()
            .expect("kline writer mutex poisoned")
            .clone()
            .context("kline store not initialized")?;
        let (respond_to, response) = oneshot::channel();
        tx.send(KlineWriteRequest { rows, respond_to })
            .context("kline sqlite writer stopped")?;
        response
            .await
            .context("kline sqlite writer dropped response")?
    }

    pub async fn query(&self, q: KlineQuery) -> Result<Vec<KlineBar>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || query_blocking(&path, q)).await?
    }
}

pub fn spawn_kline_service(
    cfg: KlineConfig,
    app_cfg: AppConfig,
    http: reqwest::Client,
    bus: EventBus,
    store: KlineStore,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = store.init().await {
            warn!(%error, "kline sqlite init failed");
            return;
        }

        if cfg.backfill_on_start
            && let Err(error) = backfill_history(&cfg, &app_cfg, &http, &store).await
        {
            warn!(%error, "kline historical backfill failed");
        }

        let (tx, rx) = mpsc::channel(4096);
        let writer_store = store.clone();
        let writer_shutdown = shutdown.clone();
        let writer = tokio::spawn(async move {
            run_kline_writer(writer_store, rx, writer_shutdown).await;
        });

        let mut rx = bus.subscribe();
        let mut aggregator = RealtimeKlineAggregator::new(cfg.intervals.clone());
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                received = rx.recv() => {
                    match received {
                        Ok(tick) => {
                            for bar in aggregator.update(tick.tick.as_ref()) {
                                if tx.try_send(bar).is_err() {
                                    warn!("kline writer channel full, dropping realtime kline update");
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "kline realtime subscriber lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        drop(tx);
        let _ = writer.await;
    })
}

async fn run_kline_writer(
    store: KlineStore,
    mut rx: mpsc::Receiver<KlineBar>,
    shutdown: CancellationToken,
) {
    let mut batch = Vec::with_capacity(WRITER_BATCH_MAX);
    let mut flush = tokio::time::interval(Duration::from_millis(WRITER_FLUSH_MS));
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = flush.tick(), if !batch.is_empty() => {
                let rows = std::mem::take(&mut batch);
                if let Err(error) = store.upsert_many(rows).await {
                    warn!(%error, "kline batch write failed");
                }
            }
            received = rx.recv() => {
                match received {
                    Some(row) => {
                        batch.push(row);
                        if batch.len() >= WRITER_BATCH_MAX {
                            let rows = std::mem::take(&mut batch);
                            if let Err(error) = store.upsert_many(rows).await {
                                warn!(%error, "kline batch write failed");
                            }
                        }
                    }
                    None => break,
                }
            }
        }
    }
    if !batch.is_empty() {
        let _ = store.upsert_many(batch).await;
    }
}

pub async fn backfill_history(
    cfg: &KlineConfig,
    app_cfg: &AppConfig,
    http: &reqwest::Client,
    store: &KlineStore,
) -> Result<()> {
    for source in &cfg.sources {
        let source = source.to_ascii_lowercase();
        for interval in &cfg.intervals {
            for symbol in &app_cfg.symbols {
                let rows = match source.as_str() {
                    "binance" => {
                        fetch_binance_klines(http, "spot", symbol, interval, cfg.history_limit)
                            .await?
                    }
                    "okx" => {
                        fetch_okx_klines(http, "spot", symbol, interval, cfg.history_limit).await?
                    }
                    _ => Vec::new(),
                };
                store.upsert_many(rows).await?;
            }
            for symbol in app_cfg.perp_symbols.as_deref().unwrap_or_default() {
                let rows = match source.as_str() {
                    "binance" => {
                        fetch_binance_klines(http, "perp", symbol, interval, cfg.history_limit)
                            .await?
                    }
                    "okx" => {
                        fetch_okx_klines(http, "perp", symbol, interval, cfg.history_limit).await?
                    }
                    _ => Vec::new(),
                };
                store.upsert_many(rows).await?;
            }
        }
    }
    info!("kline historical backfill completed");
    Ok(())
}

async fn fetch_binance_klines(
    http: &reqwest::Client,
    market: &str,
    symbol: &str,
    interval: &str,
    limit: usize,
) -> Result<Vec<KlineBar>> {
    let base = if market == "perp" {
        "https://fapi.binance.com/fapi/v1/klines"
    } else {
        "https://api.binance.com/api/v3/klines"
    };
    let limit = limit.clamp(1, 1500);
    let rows = http
        .get(base)
        .query(&[
            ("symbol", symbol),
            ("interval", interval),
            ("limit", &limit.to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Vec<Value>>>()
        .await
        .context("failed to parse binance kline payload")?;

    rows.into_iter()
        .map(|row| parse_binance_row(market, symbol, interval, row))
        .collect()
}

fn parse_binance_row(
    market: &str,
    symbol: &str,
    interval: &str,
    row: Vec<Value>,
) -> Result<KlineBar> {
    if row.len() < 6 {
        bail!("short binance kline row");
    }
    let open_time_ms = value_u64(&row[0]).context("missing open time")?;
    let interval_ms = interval_to_ms(interval).context("unsupported interval")?;
    Ok(KlineBar {
        exchange: "binance".to_string(),
        market: market.to_string(),
        symbol: symbol.to_ascii_uppercase(),
        interval: interval.to_string(),
        open_time_ms,
        close_time_ms: open_time_ms + interval_ms - 1,
        open: value_f64(&row[1]).context("missing open")?,
        high: value_f64(&row[2]).context("missing high")?,
        low: value_f64(&row[3]).context("missing low")?,
        close: value_f64(&row[4]).context("missing close")?,
        volume: value_f64(&row[5]),
        source: "historical_rest".to_string(),
        updated_at_ms: crate::types::now_ms(),
    })
}

async fn fetch_okx_klines(
    http: &reqwest::Client,
    market: &str,
    symbol: &str,
    interval: &str,
    limit: usize,
) -> Result<Vec<KlineBar>> {
    let inst_id = okx_inst_id(symbol, market);
    let limit = limit.clamp(1, 300).to_string();
    let payload = http
        .get("https://www.okx.com/api/v5/market/candles")
        .query(&[
            ("instId", inst_id.as_str()),
            ("bar", interval),
            ("limit", &limit),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse okx kline payload")?;
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    rows.into_iter()
        .map(|row| parse_okx_row(market, symbol, interval, row))
        .collect()
}

fn parse_okx_row(market: &str, symbol: &str, interval: &str, row: Value) -> Result<KlineBar> {
    let row = row.as_array().context("okx kline row is not array")?;
    if row.len() < 6 {
        bail!("short okx kline row");
    }
    let open_time_ms = value_u64(&row[0]).context("missing open time")?;
    let interval_ms = interval_to_ms(interval).context("unsupported interval")?;
    Ok(KlineBar {
        exchange: "okx".to_string(),
        market: market.to_string(),
        symbol: symbol.to_ascii_uppercase(),
        interval: interval.to_string(),
        open_time_ms,
        close_time_ms: open_time_ms + interval_ms - 1,
        open: value_f64(&row[1]).context("missing open")?,
        high: value_f64(&row[2]).context("missing high")?,
        low: value_f64(&row[3]).context("missing low")?,
        close: value_f64(&row[4]).context("missing close")?,
        volume: value_f64(&row[5]),
        source: "historical_rest".to_string(),
        updated_at_ms: crate::types::now_ms(),
    })
}

fn okx_inst_id(symbol: &str, market: &str) -> String {
    let symbol = symbol.trim().to_ascii_uppercase();
    let (base, quote) = symbol
        .strip_suffix("USDT")
        .map(|base| (base, "USDT"))
        .or_else(|| symbol.strip_suffix("USDC").map(|base| (base, "USDC")))
        .unwrap_or((symbol.as_str(), ""));
    if market == "perp" {
        format!("{base}-{quote}-SWAP")
    } else {
        format!("{base}-{quote}")
    }
}

struct RealtimeKlineAggregator {
    intervals: Vec<(String, u64)>,
    bars: HashMap<String, KlineBar>,
}

impl RealtimeKlineAggregator {
    fn new(intervals: Vec<String>) -> Self {
        let intervals = intervals
            .into_iter()
            .filter_map(|interval| interval_to_ms(&interval).map(|ms| (interval, ms)))
            .collect();
        Self {
            intervals,
            bars: HashMap::new(),
        }
    }

    fn update(&mut self, tick: &NormalizedTick) -> Vec<KlineBar> {
        let price = (tick.bid + tick.ask) / 2.0;
        if !price.is_finite() || price <= 0.0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(self.intervals.len());
        for (interval, interval_ms) in self.intervals.clone() {
            let open_time_ms = tick.ts / interval_ms * interval_ms;
            self.evict_old_bars(tick.ts, interval_ms);
            let key = format!(
                "{}:{}:{}:{}:{}",
                tick.exchange, tick.market, tick.symbol, interval, open_time_ms
            );
            let row = self.bars.entry(key).or_insert_with(|| KlineBar {
                exchange: tick.exchange.to_string(),
                market: tick.market.to_string(),
                symbol: tick.symbol.clone(),
                interval: interval.clone(),
                open_time_ms,
                close_time_ms: open_time_ms + interval_ms - 1,
                open: price,
                high: price,
                low: price,
                close: price,
                volume: None,
                source: "realtime_tick".to_string(),
                updated_at_ms: crate::types::now_ms(),
            });
            row.high = row.high.max(price);
            row.low = row.low.min(price);
            row.close = price;
            row.updated_at_ms = crate::types::now_ms();
            out.push(row.clone());
        }
        out
    }

    fn evict_old_bars(&mut self, now_ms: u64, interval_ms: u64) {
        let retention_ms = interval_ms.saturating_mul(REALTIME_BAR_RETENTION_WINDOWS);
        let min_open_time = now_ms.saturating_sub(retention_ms);
        self.bars.retain(|key, _| {
            key.rsplit(':')
                .next()
                .and_then(|x| x.parse::<u64>().ok())
                .is_none_or(|open_time| open_time >= min_open_time)
        });
    }
}

fn init_db(path: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    init_db_connection(&conn)
}

fn init_db_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS klines (
            exchange TEXT NOT NULL,
            market TEXT NOT NULL,
            symbol TEXT NOT NULL,
            interval TEXT NOT NULL,
            open_time_ms INTEGER NOT NULL,
            close_time_ms INTEGER NOT NULL,
            open REAL NOT NULL,
            high REAL NOT NULL,
            low REAL NOT NULL,
            close REAL NOT NULL,
            volume REAL,
            source TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(exchange, market, symbol, interval, open_time_ms)
        );
        CREATE INDEX IF NOT EXISTS idx_klines_lookup
            ON klines(symbol, exchange, market, interval, open_time_ms);
        "#,
    )?;
    Ok(())
}

fn run_sqlite_writer(path: String, rx: std_mpsc::Receiver<KlineWriteRequest>) {
    let mut conn = match Connection::open(&path) {
        Ok(conn) => conn,
        Err(error) => {
            let error = error.to_string();
            while let Ok(request) = rx.recv() {
                let _ = request.respond_to.send(Err(anyhow::anyhow!(
                    "kline sqlite writer init failed: {error}"
                )));
            }
            return;
        }
    };
    if let Err(error) = init_db_connection(&conn) {
        let error = error.to_string();
        while let Ok(request) = rx.recv() {
            let _ = request.respond_to.send(Err(anyhow::anyhow!(
                "kline sqlite writer init failed: {error}"
            )));
        }
        return;
    }
    while let Ok(request) = rx.recv() {
        let result = upsert_many_with_conn(&mut conn, &request.rows);
        let _ = request.respond_to.send(result);
    }
}

fn upsert_many_with_conn(conn: &mut Connection, rows: &[KlineBar]) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            r#"
            INSERT INTO klines (
                exchange, market, symbol, interval, open_time_ms, close_time_ms,
                open, high, low, close, volume, source, updated_at_ms
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(exchange, market, symbol, interval, open_time_ms)
            DO UPDATE SET
                close_time_ms=excluded.close_time_ms,
                open=excluded.open,
                high=excluded.high,
                low=excluded.low,
                close=excluded.close,
                volume=excluded.volume,
                source=excluded.source,
                updated_at_ms=excluded.updated_at_ms
            "#,
        )?;
        for row in rows {
            stmt.execute(params![
                row.exchange,
                row.market,
                row.symbol,
                row.interval,
                row.open_time_ms as i64,
                row.close_time_ms as i64,
                row.open,
                row.high,
                row.low,
                row.close,
                row.volume,
                row.source,
                row.updated_at_ms as i64,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn query_blocking(path: &str, q: KlineQuery) -> Result<Vec<KlineBar>> {
    let conn = Connection::open(path)?;
    let mut sql = String::from(
        "SELECT exchange, market, symbol, interval, open_time_ms, close_time_ms, open, high, low, close, volume, source, updated_at_ms FROM klines WHERE 1=1",
    );
    let mut bind_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    push_filter(&mut sql, &mut bind_values, "exchange", q.exchange);
    push_filter(&mut sql, &mut bind_values, "market", q.market);
    push_filter(
        &mut sql,
        &mut bind_values,
        "symbol",
        q.symbol.map(|x| x.to_ascii_uppercase()),
    );
    push_filter(&mut sql, &mut bind_values, "interval", q.interval);
    if let Some(start_ms) = q.start_ms {
        sql.push_str(" AND open_time_ms >= ?");
        bind_values.push(Box::new(start_ms as i64));
    }
    if let Some(end_ms) = q.end_ms {
        sql.push_str(" AND open_time_ms <= ?");
        bind_values.push(Box::new(end_ms as i64));
    }
    sql.push_str(" ORDER BY open_time_ms DESC LIMIT ?");
    bind_values.push(Box::new(q.limit.clamp(1, 5000) as i64));
    let refs = bind_values
        .iter()
        .map(|value| value.as_ref())
        .collect::<Vec<_>>();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(refs), |row| {
        Ok(KlineBar {
            exchange: row.get(0)?,
            market: row.get(1)?,
            symbol: row.get(2)?,
            interval: row.get(3)?,
            open_time_ms: row.get::<_, i64>(4)? as u64,
            close_time_ms: row.get::<_, i64>(5)? as u64,
            open: row.get(6)?,
            high: row.get(7)?,
            low: row.get(8)?,
            close: row.get(9)?,
            volume: row.get(10)?,
            source: row.get(11)?,
            updated_at_ms: row.get::<_, i64>(12)? as u64,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn push_filter(
    sql: &mut String,
    bind_values: &mut Vec<Box<dyn rusqlite::ToSql>>,
    column: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        sql.push_str(" AND ");
        sql.push_str(column);
        sql.push_str(" = ?");
        bind_values.push(Box::new(value));
    }
}

fn value_f64(value: &Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|x| x.parse().ok())
        .or_else(|| value.as_f64())
}

fn value_u64(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|x| x.parse().ok())
        .or_else(|| value.as_u64())
}

pub fn interval_to_ms(interval: &str) -> Option<u64> {
    match interval {
        "1m" => Some(60_000),
        "3m" => Some(180_000),
        "5m" => Some(300_000),
        "15m" => Some(900_000),
        "30m" => Some(1_800_000),
        "1h" => Some(3_600_000),
        "4h" => Some(14_400_000),
        "1d" => Some(86_400_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{KlineStore, RealtimeKlineAggregator, interval_to_ms};
    use crate::event_snapshots::NormalizedTick;

    #[test]
    fn interval_parser_supports_common_bars() {
        assert_eq!(interval_to_ms("1m"), Some(60_000));
        assert_eq!(interval_to_ms("1h"), Some(3_600_000));
        assert_eq!(interval_to_ms("nope"), None);
    }

    #[test]
    fn realtime_aggregator_updates_ohlc_from_ticks() {
        let mut agg = RealtimeKlineAggregator::new(vec!["1m".to_string()]);
        let first = NormalizedTick {
            version: "v1",
            exchange: "binance",
            market: "spot",
            symbol: "BTCUSDT".to_string(),
            bid: 99.0,
            ask: 101.0,
            mark: None,
            funding: None,
            ts: 60_001,
            source_latency_ms: 0,
            stale: false,
        };
        let second = NormalizedTick {
            bid: 109.0,
            ask: 111.0,
            ..first.clone()
        };

        let bar = agg.update(&first).pop().expect("first bar");
        assert_eq!(bar.open_time_ms, 60_000);
        assert_eq!(bar.open, 100.0);
        let bar = agg.update(&second).pop().expect("updated bar");
        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.high, 110.0);
        assert_eq!(bar.close, 110.0);
    }

    #[test]
    fn realtime_aggregator_evicts_old_bars() {
        let mut agg = RealtimeKlineAggregator::new(vec!["1m".to_string()]);
        for minute in 0..1_010_u64 {
            let ts = minute * 60_000 + 1;
            agg.update(&NormalizedTick {
                version: "v1",
                exchange: "binance",
                market: "spot",
                symbol: "BTCUSDT".to_string(),
                bid: 99.0,
                ask: 101.0,
                mark: None,
                funding: None,
                ts,
                source_latency_ms: 0,
                stale: false,
            });
        }
        assert!(agg.bars.len() <= 1_001);
    }

    #[tokio::test]
    async fn kline_store_requires_explicit_init_before_write() {
        let path = std::env::temp_dir().join(format!(
            "market_bridge_kline_init_{}.sqlite",
            crate::types::now_ms()
        ));
        let store = KlineStore::new(path.to_string_lossy().to_string());
        let row = super::KlineBar {
            exchange: "binance".to_string(),
            market: "spot".to_string(),
            symbol: "BTCUSDT".to_string(),
            interval: "1m".to_string(),
            open_time_ms: 60_000,
            close_time_ms: 119_999,
            open: 1.0,
            high: 2.0,
            low: 1.0,
            close: 2.0,
            volume: None,
            source: "test".to_string(),
            updated_at_ms: crate::types::now_ms(),
        };

        assert!(store.upsert_many(vec![row.clone()]).await.is_err());
        store.init().await.expect("init");
        store
            .upsert_many(vec![row])
            .await
            .expect("upsert after init");
        let _ = std::fs::remove_file(path);
    }
}
