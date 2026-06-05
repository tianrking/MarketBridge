use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::Url;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::config::ClickHouseConfig;
use crate::event_bus::{EventBus, SharedEvent};
use crate::types::{DataEvent, MarketKind, TradeSide};

const INSERT_MAX_ATTEMPTS: usize = 8;
const INSERT_INITIAL_BACKOFF_MS: u64 = 100;
const INSERT_MAX_BACKOFF_MS: u64 = 5_000;

pub fn spawn_clickhouse_sink(
    bus: EventBus,
    cfg: ClickHouseConfig,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if !cfg.enabled {
            return;
        }

        let sink = match ClickHouseSink::new(cfg.clone()) {
            Ok(sink) => sink,
            Err(error) => {
                error!(%error, "clickhouse sink configuration failed");
                return;
            }
        };

        if cfg.init_tables
            && let Err(error) = sink.init().await
        {
            error!(%error, "clickhouse table init failed");
            return;
        }

        let mut rx = bus.subscribe_events();
        let (local_tx, mut local_rx) =
            mpsc::channel::<Arc<SharedEvent>>(cfg.local_buffer.max(cfg.batch_max).max(1));
        let drain_shutdown = shutdown.clone();
        let drain_handle = tokio::spawn(async move {
            loop {
                let received = tokio::select! {
                    _ = drain_shutdown.cancelled() => break,
                    received = rx.recv() => received,
                };
                match received {
                    Ok(event) => {
                        if local_tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "clickhouse sink broadcast drain lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let mut batch = ClickHouseBatch::default();
        let mut flush_interval = tokio::time::interval(Duration::from_millis(cfg.flush_ms.max(1)));
        flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            let received = tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = flush_interval.tick(), if !batch.is_empty() => {
                    if let Err(error) = sink.flush(&mut batch).await {
                        error!(%error, "clickhouse sink failed after retries");
                        break;
                    }
                    continue;
                }
                received = local_rx.recv() => received,
            };

            match received {
                Some(event) => {
                    batch.push(event.as_ref());
                    if batch.len() >= cfg.batch_max.max(1)
                        && let Err(error) = sink.flush(&mut batch).await
                    {
                        error!(%error, "clickhouse sink failed after retries");
                        break;
                    }
                }
                None => break,
            }
        }

        if !batch.is_empty() {
            let _ = sink.flush(&mut batch).await;
        }
        drain_handle.abort();
    })
}

struct ClickHouseSink {
    cfg: ClickHouseConfig,
    client: reqwest::Client,
    password: Option<String>,
}

impl ClickHouseSink {
    fn new(cfg: ClickHouseConfig) -> Result<Self> {
        let _ = Url::parse(&cfg.url).context("invalid clickhouse url")?;
        let password = cfg.password.clone().or_else(|| {
            cfg.password_env
                .as_ref()
                .and_then(|env| std::env::var(env).ok())
        });
        Ok(Self {
            cfg,
            client: reqwest::Client::new(),
            password,
        })
    }

    async fn init(&self) -> Result<()> {
        self.execute(&format!(
            "CREATE DATABASE IF NOT EXISTS {}",
            ident(&self.cfg.database)?
        ))
        .await?;
        for ddl in table_ddls(&self.cfg.database)? {
            self.execute(&ddl).await?;
        }
        info!(database=%self.cfg.database, "clickhouse tables ready");
        Ok(())
    }

    async fn flush(&self, batch: &mut ClickHouseBatch) -> Result<()> {
        self.insert_table("market_quotes", &batch.market_quotes)
            .await?;
        self.insert_table("trades", &batch.trades).await?;
        self.insert_table("order_books", &batch.order_books).await?;
        self.insert_table("funding_rates", &batch.funding_rates)
            .await?;
        self.insert_table("open_interest", &batch.open_interest)
            .await?;
        self.insert_table("liquidations", &batch.liquidations)
            .await?;
        self.insert_table("external_signals", &batch.external_signals)
            .await?;
        batch.clear();
        Ok(())
    }

    async fn insert_table<T: Serialize>(&self, table: &str, rows: &[T]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut body = String::new();
        for row in rows {
            body.push_str(&serde_json::to_string(row)?);
            body.push('\n');
        }
        let sql = format!(
            "INSERT INTO {}.{} FORMAT JSONEachRow",
            ident(&self.cfg.database)?,
            ident(table)?
        );
        self.execute_body(&sql, body).await
    }

    async fn execute(&self, sql: &str) -> Result<()> {
        self.execute_body(sql, String::new()).await
    }

    async fn execute_body(&self, sql: &str, body: String) -> Result<()> {
        let mut backoff = Duration::from_millis(INSERT_INITIAL_BACKOFF_MS);
        let mut last_error = None;
        for attempt in 1..=INSERT_MAX_ATTEMPTS {
            let mut request = self
                .client
                .post(&self.cfg.url)
                .query(&[("query", sql)])
                .body(body.clone());
            if let Some(username) = &self.cfg.username {
                request = request.basic_auth(username, self.password.clone());
            }
            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        return Ok(());
                    }
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    last_error = Some(format!("http {status}: {text}"));
                }
                Err(error) => last_error = Some(error.to_string()),
            }

            warn!(
                attempt,
                sql = %short_sql(sql),
                error = %last_error.as_deref().unwrap_or("unknown"),
                "clickhouse request failed, retrying"
            );
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_millis(INSERT_MAX_BACKOFF_MS));
        }
        bail!(
            "clickhouse request failed after {INSERT_MAX_ATTEMPTS} attempts: {}",
            last_error.unwrap_or_else(|| "unknown".to_string())
        )
    }
}

#[derive(Default)]
struct ClickHouseBatch {
    market_quotes: Vec<MarketQuoteRow>,
    trades: Vec<TradeRow>,
    order_books: Vec<OrderBookRow>,
    funding_rates: Vec<FundingRateRow>,
    open_interest: Vec<OpenInterestRow>,
    liquidations: Vec<LiquidationRow>,
    external_signals: Vec<ExternalSignalRow>,
}

impl ClickHouseBatch {
    fn push(&mut self, shared: &SharedEvent) {
        let raw_json = shared.json().as_ref().to_string();
        match shared.event.as_ref() {
            DataEvent::Tick(t) => self.market_quotes.push(MarketQuoteRow {
                ts_ms: t.ts_ms,
                exchange: t.exchange,
                market: market_kind(t.market),
                symbol: t.symbol.to_string(),
                bid: t.bid,
                ask: t.ask,
                mark: t.mark,
                funding_rate: t.funding_rate,
                raw_json,
            }),
            DataEvent::Trade(t) => self.trades.push(TradeRow {
                ts_ms: t.ts_ms,
                exchange: t.exchange,
                market: market_kind(t.market),
                symbol: t.symbol.to_string(),
                price: t.price,
                qty: t.qty,
                side: trade_side(t.side),
                trade_id: t.trade_id.as_ref().map(|id| id.to_string()),
                raw_json,
            }),
            DataEvent::OrderBook(t) => self.order_books.push(OrderBookRow {
                ts_ms: t.ts_ms,
                exchange: t.exchange,
                market: market_kind(t.market),
                symbol: t.symbol.to_string(),
                best_bid: t.bids.first().map(|level| level.price),
                best_ask: t.asks.first().map(|level| level.price),
                bids_json: serde_json::to_string(&t.bids).unwrap_or_else(|_| "[]".to_string()),
                asks_json: serde_json::to_string(&t.asks).unwrap_or_else(|_| "[]".to_string()),
                last_update_id: t.last_update_id,
                raw_json,
            }),
            DataEvent::FundingRate(t) => self.funding_rates.push(FundingRateRow {
                ts_ms: t.ts_ms,
                exchange: t.exchange,
                symbol: t.symbol.to_string(),
                funding_rate: t.funding_rate,
                next_funding_time_ms: t.next_funding_time_ms,
                mark_price: t.mark_price,
                index_price: t.index_price,
                raw_json,
            }),
            DataEvent::OpenInterest(t) => self.open_interest.push(OpenInterestRow {
                ts_ms: t.ts_ms,
                exchange: t.exchange,
                symbol: t.symbol.to_string(),
                open_interest: t.open_interest,
                open_interest_value: t.open_interest_value,
                raw_json,
            }),
            DataEvent::Liquidation(t) => self.liquidations.push(LiquidationRow {
                ts_ms: t.ts_ms,
                exchange: t.exchange,
                symbol: t.symbol.to_string(),
                side: trade_side(t.side),
                price: t.price,
                qty: t.qty,
                raw_json,
            }),
            DataEvent::ExternalSignal(t) => self.external_signals.push(ExternalSignalRow {
                ts_ms: t.ts_ms,
                source: t.source,
                category: t.category.to_string(),
                symbol: t.symbol.as_ref().map(|symbol| symbol.to_string()),
                metric: t.metric.to_string(),
                value: t.value,
                score: t.score,
                title: t.title.as_ref().map(|title| title.to_string()),
                url: t.url.as_ref().map(|url| url.to_string()),
                raw_json,
            }),
            DataEvent::Heartbeat { .. } => {}
        }
    }

    fn len(&self) -> usize {
        self.market_quotes.len()
            + self.trades.len()
            + self.order_books.len()
            + self.funding_rates.len()
            + self.open_interest.len()
            + self.liquidations.len()
            + self.external_signals.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn clear(&mut self) {
        self.market_quotes.clear();
        self.trades.clear();
        self.order_books.clear();
        self.funding_rates.clear();
        self.open_interest.clear();
        self.liquidations.clear();
        self.external_signals.clear();
    }
}

#[derive(Debug, Clone, Serialize)]
struct MarketQuoteRow {
    ts_ms: u64,
    exchange: &'static str,
    market: &'static str,
    symbol: String,
    bid: f64,
    ask: f64,
    mark: Option<f64>,
    funding_rate: Option<f64>,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct TradeRow {
    ts_ms: u64,
    exchange: &'static str,
    market: &'static str,
    symbol: String,
    price: f64,
    qty: f64,
    side: &'static str,
    trade_id: Option<String>,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct OrderBookRow {
    ts_ms: u64,
    exchange: &'static str,
    market: &'static str,
    symbol: String,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    bids_json: String,
    asks_json: String,
    last_update_id: Option<u64>,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct FundingRateRow {
    ts_ms: u64,
    exchange: &'static str,
    symbol: String,
    funding_rate: f64,
    next_funding_time_ms: Option<u64>,
    mark_price: Option<f64>,
    index_price: Option<f64>,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct OpenInterestRow {
    ts_ms: u64,
    exchange: &'static str,
    symbol: String,
    open_interest: f64,
    open_interest_value: Option<f64>,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct LiquidationRow {
    ts_ms: u64,
    exchange: &'static str,
    symbol: String,
    side: &'static str,
    price: f64,
    qty: f64,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct ExternalSignalRow {
    ts_ms: u64,
    source: &'static str,
    category: String,
    symbol: Option<String>,
    metric: String,
    value: Option<f64>,
    score: Option<f64>,
    title: Option<String>,
    url: Option<String>,
    raw_json: String,
}

fn table_ddls(database: &str) -> Result<Vec<String>> {
    let db = ident(database)?;
    Ok(vec![
        format!(
            "CREATE TABLE IF NOT EXISTS {db}.market_quotes (ts_ms UInt64, exchange LowCardinality(String), market LowCardinality(String), symbol String, bid Float64, ask Float64, mark Nullable(Float64), funding_rate Nullable(Float64), raw_json String) ENGINE = MergeTree ORDER BY (symbol, exchange, market, ts_ms)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {db}.trades (ts_ms UInt64, exchange LowCardinality(String), market LowCardinality(String), symbol String, price Float64, qty Float64, side LowCardinality(String), trade_id Nullable(String), raw_json String) ENGINE = MergeTree ORDER BY (symbol, exchange, market, ts_ms)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {db}.order_books (ts_ms UInt64, exchange LowCardinality(String), market LowCardinality(String), symbol String, best_bid Nullable(Float64), best_ask Nullable(Float64), bids_json String, asks_json String, last_update_id Nullable(UInt64), raw_json String) ENGINE = MergeTree ORDER BY (symbol, exchange, market, ts_ms)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {db}.funding_rates (ts_ms UInt64, exchange LowCardinality(String), symbol String, funding_rate Float64, next_funding_time_ms Nullable(UInt64), mark_price Nullable(Float64), index_price Nullable(Float64), raw_json String) ENGINE = MergeTree ORDER BY (symbol, exchange, ts_ms)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {db}.open_interest (ts_ms UInt64, exchange LowCardinality(String), symbol String, open_interest Float64, open_interest_value Nullable(Float64), raw_json String) ENGINE = MergeTree ORDER BY (symbol, exchange, ts_ms)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {db}.liquidations (ts_ms UInt64, exchange LowCardinality(String), symbol String, side LowCardinality(String), price Float64, qty Float64, raw_json String) ENGINE = MergeTree ORDER BY (symbol, exchange, ts_ms)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {db}.external_signals (ts_ms UInt64, source LowCardinality(String), category LowCardinality(String), symbol Nullable(String), metric String, value Nullable(Float64), score Nullable(Float64), title Nullable(String), url Nullable(String), raw_json String) ENGINE = MergeTree ORDER BY (source, category, metric, ts_ms)"
        ),
    ])
}

fn ident(value: &str) -> Result<String> {
    if value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !value.is_empty() {
        Ok(value.to_string())
    } else {
        bail!("invalid clickhouse identifier: {value}")
    }
}

fn market_kind(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "perp",
    }
}

fn trade_side(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
        TradeSide::Unknown => "unknown",
    }
}

fn short_sql(sql: &str) -> String {
    const MAX: usize = 120;
    if sql.len() <= MAX {
        sql.to_string()
    } else {
        format!("{}...", &sql[..MAX])
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::event_bus::SharedEvent;
    use crate::types::{DataEvent, MarketKind, TradeTick};

    use super::{ClickHouseBatch, ident, table_ddls};

    #[test]
    fn rejects_unsafe_identifier() {
        assert!(ident("marketbridge").is_ok());
        assert!(ident("bad-name").is_err());
        assert!(ident("x;DROP").is_err());
    }

    #[test]
    fn creates_expected_table_ddls() {
        let ddls = table_ddls("marketbridge").expect("ddls");
        assert_eq!(ddls.len(), 7);
        assert!(ddls[0].contains("marketbridge.market_quotes"));
    }

    #[test]
    fn batches_trade_rows() {
        let event = Arc::new(DataEvent::Trade(TradeTick {
            exchange: "binance",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 2.0,
            side: crate::types::TradeSide::Buy,
            trade_id: Some("1".into()),
            ts_ms: 42,
        }));
        let shared = SharedEvent::new(event);
        let mut batch = ClickHouseBatch::default();
        batch.push(&shared);

        assert_eq!(batch.trades.len(), 1);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.trades[0].symbol, "BTCUSDT");
    }
}
