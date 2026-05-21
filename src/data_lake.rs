use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::klines::{KlineBar, interval_to_ms};
use crate::types::now_ms;

#[derive(Debug, Clone)]
pub struct DataLakeStore {
    root_dir: String,
    manifest_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LakePartition {
    pub id: i64,
    pub domain: String,
    pub format: String,
    pub exchange: String,
    pub market: String,
    pub symbol: String,
    pub interval: Option<String>,
    pub candle_type: Option<String>,
    pub day: String,
    pub file_path: String,
    pub rows: u64,
    pub bytes: u64,
    pub first_ts_ms: u64,
    pub last_ts_ms: u64,
    pub latest_watermark_ms: u64,
    pub gap_count: u64,
    pub duplicate_count: u64,
    pub coverage_ratio: Option<f64>,
    pub latency_p50_ms: Option<u64>,
    pub latency_p95_ms: Option<u64>,
    pub stale_count: u64,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LakeManifestQuery {
    pub domain: Option<String>,
    pub exchange: Option<String>,
    pub market: Option<String>,
    pub symbol: Option<String>,
    pub interval: Option<String>,
    pub candle_type: Option<String>,
    pub day: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LakeDeleteQuery {
    pub domain: Option<String>,
    pub exchange: Option<String>,
    pub market: Option<String>,
    pub symbol: Option<String>,
    pub interval: Option<String>,
    pub candle_type: Option<String>,
    pub day: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LakeDeleteResult {
    pub deleted_partitions: usize,
    pub deleted_files: usize,
    pub freed_bytes: u64,
}

#[derive(Debug, Clone)]
struct PartitionKey {
    domain: String,
    exchange: String,
    market: String,
    symbol: String,
    interval: String,
    candle_type: String,
    day: String,
}

#[derive(Debug, Clone)]
struct PartitionWrite {
    key: PartitionKey,
    rows: Vec<KlineBar>,
}

#[derive(Debug, Clone)]
struct QualityStats {
    rows: u64,
    first_ts_ms: u64,
    last_ts_ms: u64,
    latest_watermark_ms: u64,
    gap_count: u64,
    duplicate_count: u64,
    coverage_ratio: Option<f64>,
    latency_p50_ms: Option<u64>,
    latency_p95_ms: Option<u64>,
    stale_count: u64,
}

impl DataLakeStore {
    pub fn new(root_dir: impl Into<String>, manifest_path: impl Into<String>) -> Self {
        Self {
            root_dir: root_dir.into(),
            manifest_path: manifest_path.into(),
        }
    }

    pub async fn init(&self) -> Result<()> {
        let root_dir = self.root_dir.clone();
        let manifest_path = self.manifest_path.clone();
        tokio::task::spawn_blocking(move || init_lake(&root_dir, &manifest_path)).await?
    }

    pub async fn persist_klines(&self, rows: Vec<KlineBar>, candle_type: String) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let root_dir = self.root_dir.clone();
        let manifest_path = self.manifest_path.clone();
        tokio::task::spawn_blocking(move || {
            persist_klines_blocking(&root_dir, &manifest_path, rows, candle_type)
        })
        .await?
    }

    pub async fn manifest(&self, q: LakeManifestQuery) -> Result<Vec<LakePartition>> {
        let manifest_path = self.manifest_path.clone();
        tokio::task::spawn_blocking(move || manifest_blocking(&manifest_path, q)).await?
    }

    pub async fn delete_partitions(&self, q: LakeDeleteQuery) -> Result<LakeDeleteResult> {
        let root_dir = self.root_dir.clone();
        let manifest_path = self.manifest_path.clone();
        tokio::task::spawn_blocking(move || {
            delete_partitions_blocking(&root_dir, &manifest_path, q)
        })
        .await?
    }
}

fn init_lake(root_dir: &str, manifest_path: &str) -> Result<()> {
    std::fs::create_dir_all(root_dir)?;
    if let Some(parent) = Path::new(manifest_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(manifest_path)?;
    init_lake_connection(&conn)
}

fn init_lake_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS lake_partitions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            domain TEXT NOT NULL,
            format TEXT NOT NULL,
            exchange TEXT NOT NULL,
            market TEXT NOT NULL,
            symbol TEXT NOT NULL,
            interval TEXT,
            candle_type TEXT,
            day TEXT NOT NULL,
            file_path TEXT NOT NULL UNIQUE,
            rows INTEGER NOT NULL,
            bytes INTEGER NOT NULL,
            first_ts_ms INTEGER NOT NULL,
            last_ts_ms INTEGER NOT NULL,
            latest_watermark_ms INTEGER NOT NULL,
            gap_count INTEGER NOT NULL,
            duplicate_count INTEGER NOT NULL,
            coverage_ratio REAL,
            latency_p50_ms INTEGER,
            latency_p95_ms INTEGER,
            stale_count INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_lake_partitions_lookup
            ON lake_partitions(domain, exchange, market, symbol, interval, candle_type, day);
        "#,
    )?;
    Ok(())
}

fn persist_klines_blocking(
    root_dir: &str,
    manifest_path: &str,
    rows: Vec<KlineBar>,
    candle_type: String,
) -> Result<usize> {
    init_lake(root_dir, manifest_path)?;
    let partitions = partition_klines(rows, candle_type);
    let mut conn = Connection::open(manifest_path)?;
    let mut written = 0;
    for partition in partitions {
        let path = partition_path(root_dir, &partition.key, "jsonl");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stats = quality_stats(&partition.rows, &partition.key.interval);
        write_jsonl(&path, &partition.rows)?;
        let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        upsert_manifest(&mut conn, &partition.key, &path, "jsonl", bytes, &stats)?;
        written += 1;
    }
    Ok(written)
}

fn partition_klines(rows: Vec<KlineBar>, candle_type: String) -> Vec<PartitionWrite> {
    let mut grouped = HashMap::<String, PartitionWrite>::new();
    for row in rows {
        let key = PartitionKey {
            domain: "candles".to_string(),
            exchange: row.exchange.to_ascii_lowercase(),
            market: row.market.to_ascii_lowercase(),
            symbol: row.symbol.to_ascii_uppercase(),
            interval: row.interval.clone(),
            candle_type: candle_type.trim().to_ascii_lowercase(),
            day: utc_day(row.open_time_ms),
        };
        let map_key = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            key.domain,
            key.exchange,
            key.market,
            key.symbol,
            key.interval,
            key.candle_type,
            key.day
        );
        grouped
            .entry(map_key)
            .or_insert_with(|| PartitionWrite {
                key,
                rows: Vec::new(),
            })
            .rows
            .push(row);
    }
    grouped.into_values().collect()
}

fn write_jsonl(path: &Path, rows: &[KlineBar]) -> Result<()> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut writer = BufWriter::new(file);
    for row in rows {
        serde_json::to_writer(&mut writer, row)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

fn quality_stats(rows: &[KlineBar], interval: &str) -> QualityStats {
    let mut sorted = rows.to_vec();
    sorted.sort_by_key(|row| row.open_time_ms);
    let first_ts_ms = sorted.first().map(|row| row.open_time_ms).unwrap_or(0);
    let last_ts_ms = sorted.last().map(|row| row.open_time_ms).unwrap_or(0);
    let latest_watermark_ms = sorted
        .iter()
        .map(|row| row.updated_at_ms)
        .max()
        .unwrap_or(last_ts_ms);
    let interval_ms = interval_to_ms(interval);
    let mut seen = HashSet::new();
    let mut duplicate_count = 0;
    for row in &sorted {
        if !seen.insert(row.open_time_ms) {
            duplicate_count += 1;
        }
    }
    let gap_count = interval_ms
        .map(|interval_ms| {
            sorted
                .windows(2)
                .filter(|window| {
                    window[1]
                        .open_time_ms
                        .saturating_sub(window[0].open_time_ms)
                        > interval_ms
                })
                .map(|window| {
                    let delta = window[1]
                        .open_time_ms
                        .saturating_sub(window[0].open_time_ms);
                    delta / interval_ms - 1
                })
                .sum::<u64>()
        })
        .unwrap_or(0);
    let coverage_ratio = interval_ms.and_then(|interval_ms| {
        if last_ts_ms < first_ts_ms {
            return None;
        }
        let expected = ((last_ts_ms - first_ts_ms) / interval_ms + 1).max(1);
        Some((seen.len() as f64 / expected as f64).min(1.0))
    });
    let mut latencies = sorted
        .iter()
        .map(|row| row.updated_at_ms.saturating_sub(row.close_time_ms))
        .collect::<Vec<_>>();
    latencies.sort_unstable();
    let latency_p50_ms = percentile(&latencies, 50);
    let latency_p95_ms = percentile(&latencies, 95);
    let stale_cutoff_ms = interval_ms.unwrap_or(60_000).saturating_mul(2);
    let stale_count = latencies
        .iter()
        .filter(|latency| **latency > stale_cutoff_ms)
        .count() as u64;

    QualityStats {
        rows: sorted.len() as u64,
        first_ts_ms,
        last_ts_ms,
        latest_watermark_ms,
        gap_count,
        duplicate_count,
        coverage_ratio,
        latency_p50_ms,
        latency_p95_ms,
        stale_count,
    }
}

fn percentile(values: &[u64], pct: usize) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let idx = ((values.len() - 1) * pct.min(100)) / 100;
    values.get(idx).copied()
}

fn upsert_manifest(
    conn: &mut Connection,
    key: &PartitionKey,
    path: &Path,
    format: &str,
    bytes: u64,
    stats: &QualityStats,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO lake_partitions (
            domain, format, exchange, market, symbol, interval, candle_type, day,
            file_path, rows, bytes, first_ts_ms, last_ts_ms, latest_watermark_ms,
            gap_count, duplicate_count, coverage_ratio, latency_p50_ms, latency_p95_ms,
            stale_count, created_at_ms
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
        ON CONFLICT(file_path) DO UPDATE SET
            rows=lake_partitions.rows + excluded.rows,
            bytes=excluded.bytes,
            first_ts_ms=MIN(lake_partitions.first_ts_ms, excluded.first_ts_ms),
            last_ts_ms=MAX(lake_partitions.last_ts_ms, excluded.last_ts_ms),
            latest_watermark_ms=MAX(lake_partitions.latest_watermark_ms, excluded.latest_watermark_ms),
            gap_count=excluded.gap_count,
            duplicate_count=lake_partitions.duplicate_count + excluded.duplicate_count,
            coverage_ratio=excluded.coverage_ratio,
            latency_p50_ms=excluded.latency_p50_ms,
            latency_p95_ms=excluded.latency_p95_ms,
            stale_count=lake_partitions.stale_count + excluded.stale_count
        "#,
        params![
            key.domain,
            format,
            key.exchange,
            key.market,
            key.symbol,
            key.interval,
            key.candle_type,
            key.day,
            path.to_string_lossy(),
            stats.rows as i64,
            bytes as i64,
            stats.first_ts_ms as i64,
            stats.last_ts_ms as i64,
            stats.latest_watermark_ms as i64,
            stats.gap_count as i64,
            stats.duplicate_count as i64,
            stats.coverage_ratio,
            stats.latency_p50_ms.map(|value| value as i64),
            stats.latency_p95_ms.map(|value| value as i64),
            stats.stale_count as i64,
            now_ms() as i64,
        ],
    )?;
    Ok(())
}

fn manifest_blocking(manifest_path: &str, q: LakeManifestQuery) -> Result<Vec<LakePartition>> {
    let conn = Connection::open(manifest_path)?;
    init_lake_connection(&conn)?;
    let mut sql = String::from(
        "SELECT id, domain, format, exchange, market, symbol, interval, candle_type, day, file_path, rows, bytes, first_ts_ms, last_ts_ms, latest_watermark_ms, gap_count, duplicate_count, coverage_ratio, latency_p50_ms, latency_p95_ms, stale_count, created_at_ms FROM lake_partitions WHERE 1=1",
    );
    let mut bind_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    push_filter(&mut sql, &mut bind_values, "domain", q.domain);
    push_filter(
        &mut sql,
        &mut bind_values,
        "exchange",
        q.exchange.map(|v| v.to_ascii_lowercase()),
    );
    push_filter(
        &mut sql,
        &mut bind_values,
        "market",
        q.market.map(|v| v.to_ascii_lowercase()),
    );
    push_filter(
        &mut sql,
        &mut bind_values,
        "symbol",
        q.symbol.map(|v| v.to_ascii_uppercase()),
    );
    push_filter(&mut sql, &mut bind_values, "interval", q.interval);
    push_filter(
        &mut sql,
        &mut bind_values,
        "candle_type",
        q.candle_type.map(|v| v.to_ascii_lowercase()),
    );
    push_filter(&mut sql, &mut bind_values, "day", q.day);
    sql.push_str(" ORDER BY created_at_ms DESC LIMIT ?");
    bind_values.push(Box::new(q.limit.unwrap_or(200).clamp(1, 5000) as i64));
    query_manifest(&conn, &sql, bind_values)
}

fn delete_partitions_blocking(
    root_dir: &str,
    manifest_path: &str,
    q: LakeDeleteQuery,
) -> Result<LakeDeleteResult> {
    if all_delete_filters_empty(&q) {
        bail!("refusing to delete without at least one filter");
    }
    let mut conn = Connection::open(manifest_path)?;
    init_lake_connection(&conn)?;
    let mut sql = String::from(
        "SELECT id, domain, format, exchange, market, symbol, interval, candle_type, day, file_path, rows, bytes, first_ts_ms, last_ts_ms, latest_watermark_ms, gap_count, duplicate_count, coverage_ratio, latency_p50_ms, latency_p95_ms, stale_count, created_at_ms FROM lake_partitions WHERE 1=1",
    );
    let mut bind_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    push_filter(&mut sql, &mut bind_values, "domain", q.domain);
    push_filter(
        &mut sql,
        &mut bind_values,
        "exchange",
        q.exchange.map(|v| v.to_ascii_lowercase()),
    );
    push_filter(
        &mut sql,
        &mut bind_values,
        "market",
        q.market.map(|v| v.to_ascii_lowercase()),
    );
    push_filter(
        &mut sql,
        &mut bind_values,
        "symbol",
        q.symbol.map(|v| v.to_ascii_uppercase()),
    );
    push_filter(&mut sql, &mut bind_values, "interval", q.interval);
    push_filter(
        &mut sql,
        &mut bind_values,
        "candle_type",
        q.candle_type.map(|v| v.to_ascii_lowercase()),
    );
    push_filter(&mut sql, &mut bind_values, "day", q.day);
    let rows = query_manifest(&conn, &sql, bind_values)?;
    let root = Path::new(root_dir)
        .canonicalize()
        .with_context(|| format!("invalid lake root: {root_dir}"))?;
    let mut deleted_files = 0;
    let mut freed_bytes = 0;
    let tx = conn.transaction()?;
    for row in &rows {
        let path = PathBuf::from(&row.file_path);
        if path.exists() {
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(&root) {
                bail!("manifest path outside lake root: {}", row.file_path);
            }
            let bytes = std::fs::metadata(&canonical)
                .map(|meta| meta.len())
                .unwrap_or(row.bytes);
            std::fs::remove_file(&canonical)?;
            deleted_files += 1;
            freed_bytes += bytes;
        }
        tx.execute("DELETE FROM lake_partitions WHERE id = ?", params![row.id])?;
    }
    tx.commit()?;
    Ok(LakeDeleteResult {
        deleted_partitions: rows.len(),
        deleted_files,
        freed_bytes,
    })
}

fn query_manifest(
    conn: &Connection,
    sql: &str,
    bind_values: Vec<Box<dyn rusqlite::ToSql>>,
) -> Result<Vec<LakePartition>> {
    let refs = bind_values
        .iter()
        .map(|value| value.as_ref())
        .collect::<Vec<_>>();
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(refs), |row| {
        Ok(LakePartition {
            id: row.get(0)?,
            domain: row.get(1)?,
            format: row.get(2)?,
            exchange: row.get(3)?,
            market: row.get(4)?,
            symbol: row.get(5)?,
            interval: row.get(6)?,
            candle_type: row.get(7)?,
            day: row.get(8)?,
            file_path: row.get(9)?,
            rows: row.get::<_, i64>(10)? as u64,
            bytes: row.get::<_, i64>(11)? as u64,
            first_ts_ms: row.get::<_, i64>(12)? as u64,
            last_ts_ms: row.get::<_, i64>(13)? as u64,
            latest_watermark_ms: row.get::<_, i64>(14)? as u64,
            gap_count: row.get::<_, i64>(15)? as u64,
            duplicate_count: row.get::<_, i64>(16)? as u64,
            coverage_ratio: row.get(17)?,
            latency_p50_ms: row.get::<_, Option<i64>>(18)?.map(|value| value as u64),
            latency_p95_ms: row.get::<_, Option<i64>>(19)?.map(|value| value as u64),
            stale_count: row.get::<_, i64>(20)? as u64,
            created_at_ms: row.get::<_, i64>(21)? as u64,
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
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        sql.push_str(" AND ");
        sql.push_str(column);
        sql.push_str(" = ?");
        bind_values.push(Box::new(value));
    }
}

fn all_delete_filters_empty(q: &LakeDeleteQuery) -> bool {
    q.domain.is_none()
        && q.exchange.is_none()
        && q.market.is_none()
        && q.symbol.is_none()
        && q.interval.is_none()
        && q.candle_type.is_none()
        && q.day.is_none()
}

fn partition_path(root_dir: &str, key: &PartitionKey, extension: &str) -> PathBuf {
    Path::new(root_dir)
        .join(format!("domain={}", sanitize(&key.domain)))
        .join(format!("exchange={}", sanitize(&key.exchange)))
        .join(format!("market={}", sanitize(&key.market)))
        .join(format!("symbol={}", sanitize(&key.symbol)))
        .join(format!("interval={}", sanitize(&key.interval)))
        .join(format!("candle_type={}", sanitize(&key.candle_type)))
        .join(format!("day={}", sanitize(&key.day)))
        .join(format!("part-{}.{}", now_ms(), extension))
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn utc_day(ts_ms: u64) -> String {
    let days = (ts_ms / 86_400_000) as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_day_formats_unix_epoch() {
        assert_eq!(utc_day(0), "1970-01-01");
        assert_eq!(utc_day(86_400_000), "1970-01-02");
    }

    #[test]
    fn quality_stats_counts_gaps_and_duplicates() {
        let rows = vec![
            bar(0, 59_999),
            bar(60_000, 119_999),
            bar(60_000, 119_999),
            bar(180_000, 239_999),
        ];
        let stats = quality_stats(&rows, "1m");
        assert_eq!(stats.rows, 4);
        assert_eq!(stats.duplicate_count, 1);
        assert_eq!(stats.gap_count, 1);
        assert_eq!(stats.coverage_ratio, Some(0.75));
    }

    fn bar(open_time_ms: u64, close_time_ms: u64) -> KlineBar {
        KlineBar {
            exchange: "binance".to_string(),
            market: "spot".to_string(),
            symbol: "BTCUSDT".to_string(),
            interval: "1m".to_string(),
            open_time_ms,
            close_time_ms,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: Some(1.0),
            source: "test".to_string(),
            updated_at_ms: close_time_ms,
        }
    }
}
