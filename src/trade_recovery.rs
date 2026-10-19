//! Durable bounded native-trade gap repair. Late history is never injected into
//! live cumulative state. Only a closed native window can expose recovered data.
use crate::{
    ingestion::NativeCheckpoint,
    outbox::Outbox,
    types::{TradeSide, now_ms},
    window_recovery::{WindowAccumulator, WindowRequest, WindowTrade},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoveryConfig {
    pub enabled: bool,
    pub path: String,
    pub max_jobs: usize,
    pub max_deferred_streams: usize,
    pub max_rows_per_job: usize,
    pub max_evidence_bytes: u64,
    pub max_database_bytes: u64,
    pub window_ms: u64,
    pub max_window_ms: u64,
    pub max_requests_per_attempt: usize,
    pub request_timeout_ms: u64,
    pub max_attempts: u32,
}
impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "data/trade-recovery.sqlite".into(),
            max_jobs: 256,
            max_deferred_streams: 4096,
            max_rows_per_job: 100000,
            max_evidence_bytes: 128 * 1024 * 1024,
            max_database_bytes: 256 * 1024 * 1024,
            window_ms: 60000,
            max_window_ms: 15 * 60000,
            max_requests_per_attempt: 256,
            request_timeout_ms: 10000,
            max_attempts: 1000,
        }
    }
}
impl RecoveryConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.path.is_empty()
                && (1..=4096).contains(&self.max_jobs)
                && (1..=65536).contains(&self.max_deferred_streams)
                && (2..=1000000).contains(&self.max_rows_per_job)
                && self.max_evidence_bytes >= 65536
                && self.max_database_bytes >= 4 * 1024 * 1024
                && self.max_evidence_bytes <= self.max_database_bytes / 2,
            "invalid trade recovery budgets"
        );
        ensure!(
            self.window_ms >= 1000
                && self.window_ms <= 3600000
                && self.max_window_ms >= self.window_ms
                && self.max_window_ms <= 24 * 3600000
                && (1..=4096).contains(&self.max_requests_per_attempt)
                && (100..=60000).contains(&self.request_timeout_ms)
                && self.max_attempts > 0,
            "invalid recovery window/request limits"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    pub before: NativeCheckpoint,
    pub after: NativeCheckpoint,
    pub scope: WindowRequest,
}
#[derive(Debug, Clone)]
struct Job {
    id: u64,
    gap: Gap,
    attempt: u32,
}
#[derive(Clone)]
pub struct RecoveryStore {
    pub backend: Outbox,
    pub config: RecoveryConfig,
}

fn crc(bytes: &[u8]) -> u32 {
    let mut c = flate2::Crc::new();
    c.update(bytes);
    c.sum()
}

pub(crate) fn schedule(
    tx: &Transaction<'_>,
    before: &NativeCheckpoint,
    after: &NativeCheckpoint,
    cfg: &RecoveryConfig,
) -> Result<()> {
    if !before.key.starts_with("binance:trade:")
        || before
            .sequence
            .checked_add(1)
            .is_none_or(|next| after.sequence <= next)
    {
        return Ok(());
    }
    ensure!(before.key == after.key, "repair checkpoint stream mismatch");
    let parts = before.key.splitn(4, ':').collect::<Vec<_>>();
    ensure!(parts.len() == 4, "invalid native recovery key");
    let start = before.signature[2] / cfg.window_ms * cfg.window_ms;
    let end = (after.signature[2] / cfg.window_ms)
        .checked_add(1)
        .and_then(|n| n.checked_mul(cfg.window_ms))
        .context("repair window overflow")?;
    let gap = Gap {
        before: before.clone(),
        after: after.clone(),
        scope: WindowRequest {
            exchange: "binance".into(),
            market: parts[2].into(),
            symbol: parts[3].into(),
            start_ms: start,
            end_ms: end,
        },
    };
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recovery_jobs WHERE stream=?1 AND before_id=?2 AND after_id=?3)",params![before.key,before.sequence,after.sequence],|r|r.get(0))?;
    if exists {
        return Ok(());
    }
    while tx.query_row::<usize, _, _>("SELECT COUNT(*) FROM recovery_jobs", [], |r| r.get(0))?
        >= cfg.max_jobs
    {
        if !evict_one(tx, None)? {
            defer_gap(tx, &gap, cfg)?;
            return Ok(());
        }
    }
    let body = serde_json::to_string(&gap)?;
    tx.execute("INSERT INTO recovery_jobs(stream,before_id,after_id,body,crc32,state,attempt,next_ms,bytes) VALUES(?1,?2,?3,?4,?5,'pending',0,?6,0)",params![before.key,before.sequence,after.sequence,body,crc(body.as_bytes()),now_ms()])?;
    Ok(())
}
fn defer_gap(tx: &Transaction<'_>, gap: &Gap, cfg: &RecoveryConfig) -> Result<()> {
    let prior: Option<(String, u32)> = tx
        .query_row(
            "SELECT body,crc32 FROM recovery_debt WHERE stream=?1",
            [&gap.before.key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let mut merged = gap.clone();
    if let Some((body, checksum)) = prior {
        ensure!(
            crc(body.as_bytes()) == checksum,
            "deferred recovery CRC mismatch"
        );
        let previous: Gap = serde_json::from_str(&body)?;
        merged.before = previous.before;
        merged.scope.start_ms = previous.scope.start_ms.min(gap.scope.start_ms);
    } else {
        let count: usize = tx.query_row("SELECT COUNT(*) FROM recovery_debt", [], |r| r.get(0))?;
        ensure!(
            count < cfg.max_deferred_streams,
            "durable deferred recovery stream budget exhausted"
        );
    }
    let body = serde_json::to_string(&merged)?;
    tx.execute("INSERT INTO recovery_debt(stream,body,crc32) VALUES(?1,?2,?3) ON CONFLICT(stream) DO UPDATE SET body=excluded.body,crc32=excluded.crc32",params![merged.before.key,body,crc(body.as_bytes())])?;
    tx.execute(
        "UPDATE recovery_meta SET rejected_jobs=rejected_jobs+1 WHERE id=1",
        [],
    )?;
    Ok(())
}
fn promote_debt(tx: &Transaction<'_>, cfg: &RecoveryConfig) -> Result<()> {
    let row: Option<(String, String, u32)> = tx
        .query_row(
            "SELECT stream,body,crc32 FROM recovery_debt ORDER BY stream LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((stream, body, checksum)) = row else {
        return Ok(());
    };
    while tx.query_row::<usize, _, _>("SELECT COUNT(*) FROM recovery_jobs", [], |r| r.get(0))?
        >= cfg.max_jobs
    {
        if !evict_one(tx, None)? {
            return Ok(());
        }
    }
    ensure!(
        crc(body.as_bytes()) == checksum,
        "deferred gap CRC mismatch"
    );
    let gap: Gap = serde_json::from_str(&body)?;
    schedule(tx, &gap.before, &gap.after, cfg)?;
    tx.execute("DELETE FROM recovery_debt WHERE stream=?1", [stream])?;
    Ok(())
}
fn evict_one(tx: &Transaction<'_>, exclude: Option<u64>) -> Result<bool> {
    let row:Option<(u64,u64)>=tx.query_row("SELECT id,bytes FROM recovery_jobs WHERE state IN ('verified','blocked') AND id!=?1 ORDER BY id LIMIT 1",[exclude.unwrap_or(0)],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((id, bytes)) = row else {
        return Ok(false);
    };
    tx.execute("DELETE FROM recovery_records WHERE job_id=?1", [id])?;
    tx.execute("DELETE FROM recovery_jobs WHERE id=?1", [id])?;
    tx.execute("UPDATE recovery_meta SET bytes=bytes-?1,evicted_jobs=evicted_jobs+1,last_evicted_id=?2 WHERE id=1",params![bytes,id])?;
    Ok(true)
}
impl RecoveryStore {
    pub async fn initialize(backend: Outbox, config: RecoveryConfig) -> Result<Self> {
        config.validate()?;
        backend.blocking(|conn| {
            conn.execute_batch("CREATE TABLE IF NOT EXISTS recovery_meta(id INTEGER PRIMARY KEY CHECK(id=1),bytes INTEGER NOT NULL,rejected_jobs INTEGER NOT NULL,evicted_jobs INTEGER NOT NULL,last_evicted_id INTEGER NOT NULL);
            INSERT OR IGNORE INTO recovery_meta VALUES(1,0,0,0,0);
            CREATE TABLE IF NOT EXISTS recovery_jobs(id INTEGER PRIMARY KEY AUTOINCREMENT,stream TEXT NOT NULL,before_id INTEGER NOT NULL,after_id INTEGER NOT NULL,body TEXT NOT NULL,crc32 INTEGER NOT NULL,state TEXT NOT NULL,attempt INTEGER NOT NULL,next_ms INTEGER NOT NULL,error TEXT,report TEXT,report_crc32 INTEGER,bytes INTEGER NOT NULL,UNIQUE(stream,before_id,after_id));
            CREATE TABLE IF NOT EXISTS recovery_records(job_id INTEGER NOT NULL,sequence INTEGER NOT NULL,body TEXT NOT NULL,crc32 INTEGER NOT NULL,PRIMARY KEY(job_id,sequence));
            CREATE TABLE IF NOT EXISTS recovery_debt(stream TEXT PRIMARY KEY,body TEXT NOT NULL,crc32 INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS recovery_due ON recovery_jobs(state,next_ms);
            UPDATE recovery_jobs SET state='pending' WHERE state='running';")?;
            let saved:u64=conn.query_row("SELECT bytes FROM recovery_meta WHERE id=1",[],|r|r.get(0))?;
            let actual:u64=conn.query_row("SELECT COALESCE(SUM(bytes),0) FROM recovery_jobs",[],|r|r.get(0))?;ensure!(saved==actual,"recovery byte accounting corrupt");Ok(())
        }).await?;
        backend.enable_recovery(config.clone())?;
        Ok(Self { backend, config })
    }
    pub async fn retry_job(&self, id: u64) -> Result<()> {
        self.backend.blocking(move|conn|{let (state,body,checksum):(String,String,u32)=conn.query_row("SELECT state,body,crc32 FROM recovery_jobs WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;ensure!(crc(body.as_bytes())==checksum,"job CRC mismatch");ensure!(state!="verified","verified evidence is immutable; retry only an unverified task");conn.execute("UPDATE recovery_jobs SET state='pending',attempt=0,error=NULL,next_ms=?2 WHERE id=?1",params![id,now_ms()])?;Ok(())}).await?;
        self.backend.notify_changed();
        Ok(())
    }
    pub fn inspect_status(
        conn: &rusqlite::Connection,
        config: &RecoveryConfig,
    ) -> Result<serde_json::Value> {
        let origin: String =
            conn.query_row("SELECT origin FROM outbox_state WHERE id=1", [], |r| {
                r.get(0)
            })?;
        status_response(conn, config, &origin)
    }
    pub async fn status(&self) -> Result<serde_json::Value> {
        let config = self.config.clone();
        let origin = self.backend.origin().to_string();
        self.backend
            .blocking(move |conn| status_response(conn, &config, &origin))
            .await
    }
    async fn claim(&self) -> Result<Option<Job>> {
        let config = self.config.clone();
        self.backend.blocking(move |conn| {
            let tx=conn.transaction()?;promote_debt(&tx,&config)?;let row:Option<(u64,String,u32,u32)>=tx.query_row("SELECT id,body,crc32,attempt FROM recovery_jobs WHERE state IN ('pending','retry_wait','await_boundary') AND next_ms<=?1 ORDER BY next_ms,id LIMIT 1",[now_ms()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            let job=if let Some((id,body,checksum,attempt))=row {ensure!(crc(body.as_bytes())==checksum,"recovery job CRC mismatch");let gap:Gap=serde_json::from_str(&body)?;tx.execute("UPDATE recovery_jobs SET state='running',attempt=attempt+1 WHERE id=?1",[id])?;Some(Job{id,gap,attempt:attempt+1})}else{None};tx.commit()?;Ok(job)
        }).await
    }
    async fn defer(&self, job: &Job, state: &str, error: &str, next: u64) -> Result<()> {
        let (id, state, error) = (job.id, state.to_string(), error.to_string());
        self.backend
            .blocking(move |conn| {
                conn.execute(
                    "UPDATE recovery_jobs SET state=?2,error=?3,next_ms=?4 WHERE id=?1",
                    params![id, state, error, next],
                )?;
                Ok(())
            })
            .await
    }
    async fn clear_staging(&self, id: u64) -> Result<()> {
        self.backend
            .blocking(move |conn| {
                let tx = conn.transaction()?;
                let bytes: u64 =
                    tx.query_row("SELECT bytes FROM recovery_jobs WHERE id=?1", [id], |r| {
                        r.get(0)
                    })?;
                tx.execute("DELETE FROM recovery_records WHERE job_id=?1", [id])?;
                tx.execute(
                    "UPDATE recovery_meta SET bytes=bytes-?1 WHERE id=1",
                    [bytes],
                )?;
                tx.execute(
                    "UPDATE recovery_jobs SET bytes=0,report=NULL,report_crc32=NULL WHERE id=?1",
                    [id],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await
    }
    async fn stage(&self, id: u64, rows: Vec<RecoveredTrade>) -> Result<()> {
        let budget = self.config.max_evidence_bytes;
        self.backend.blocking(move |conn|{let tx=conn.transaction()?;let mut added=0u64;for row in rows {let body=serde_json::to_string(&row)?;added+=body.len() as u64+128;
            tx.execute("INSERT INTO recovery_records(job_id,sequence,body,crc32) VALUES(?1,?2,?3,?4)",params![id,row.sequence,body,record_crc(id,row.sequence,&body)])?;}
            loop {let used:u64=tx.query_row("SELECT bytes FROM recovery_meta WHERE id=1",[],|r|r.get(0))?;if used+added<=budget {break;}
            if !evict_one(&tx,Some(id))? {return Err(permanent("global_recovery_evidence_budget_exhausted"));}}
            tx.execute("UPDATE recovery_jobs SET bytes=bytes+?2 WHERE id=?1",params![id,added])?;tx.execute("UPDATE recovery_meta SET bytes=bytes+?1 WHERE id=1",[added])?;tx.commit()?;Ok(())}).await
    }
    async fn verify_and_seal(&self, job: &Job) -> Result<()> {
        let job = job.clone();
        self.backend.blocking(move |conn|{
            let tx=conn.transaction()?;let recovery=verify_rows(&tx,job.id,job.gap.scope.clone())?;if recovery["quality"]["verified"]!=true {return Err(permanent("recovered_window_cannot_be_certified"));}
            let report=serde_json::json!({"model_version":"native-gap-recovery/v1","job_id":job.id,"gap":job.gap,"window":recovery,"global_quality_cleared":false,"historical_delivery":"separate verified history; never inserted into live cumulative flow"});let body=serde_json::to_string(&report)?;
            tx.execute("UPDATE recovery_jobs SET state='verified',error=NULL,report=?2,report_crc32=?3 WHERE id=?1",params![job.id,body,crc(body.as_bytes())])?;tx.commit()?;Ok(())
        }).await
    }
    pub async fn recover_window(&self, scope: WindowRequest) -> Result<Option<serde_json::Value>> {
        scope.validate()?;
        self.backend.blocking(move |conn|{
            let mut stmt=conn.prepare("SELECT id,body,crc32,report,report_crc32 FROM recovery_jobs WHERE state='verified' ORDER BY id DESC")?;
            for row in stmt.query_map([],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?,r.get::<_,u32>(2)?,r.get::<_,String>(3)?,r.get::<_,u32>(4)?)))? {
                let (id,body,checksum,report,report_crc)=row?;ensure!(crc(body.as_bytes())==checksum && crc(report.as_bytes())==report_crc,"recovery metadata CRC mismatch");let gap:Gap=serde_json::from_str(&body)?;
                if gap.scope.exchange==scope.exchange && gap.scope.market==scope.market && gap.scope.symbol.eq_ignore_ascii_case(&scope.symbol) && scope.start_ms>=gap.scope.start_ms && scope.end_ms<=gap.scope.end_ms {
                    let result=verify_rows(conn,id,scope.clone())?;return Ok(Some(serde_json::json!({"model_version":"native-gap-recovery/v1","job_id":id,"window":result,"global_quality_cleared":false,"frozen_native_boundaries":{"before":gap.before,"after":gap.after},"source_window":gap.scope,"provenance":"durable native-ID backfill verified against frozen live boundaries"})));
                }
            }Ok(None)
        }).await
    }
}
fn status_response(
    conn: &rusqlite::Connection,
    config: &RecoveryConfig,
    origin: &str,
) -> Result<serde_json::Value> {
    let (bytes, rejected, evicted, last): (u64, u64, u64, u64) = conn.query_row(
        "SELECT bytes,rejected_jobs,evicted_jobs,last_evicted_id FROM recovery_meta WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let mut stmt=conn.prepare("SELECT id,body,crc32,state,attempt,next_ms,error,report,report_crc32 FROM recovery_jobs ORDER BY id DESC LIMIT ?1")?;
    let mut jobs = vec![];
    for row in stmt.query_map([config.max_jobs], |r| {
        Ok((
            r.get::<_, u64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, u32>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, u32>(4)?,
            r.get::<_, u64>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<u32>>(8)?,
        ))
    })? {
        let (id, body, checksum, state, attempt, next, error, report, report_crc) = row?;
        ensure!(
            crc(body.as_bytes()) == checksum,
            "recovery job CRC mismatch"
        );
        let gap: Gap = serde_json::from_str(&body)?;
        if let Some(body) = &report {
            ensure!(
                Some(crc(body.as_bytes())) == report_crc,
                "recovery report CRC mismatch"
            );
        }
        jobs.push(serde_json::json!({"id":id,"scope":gap.scope,"candidate_from_sequence":gap.before.sequence+1,"candidate_to_sequence":gap.after.sequence-1,"range_may_include_observed_trades":true,"state":state,"attempt":attempt,"next_attempt_ms":next,"error":error,"verified":state=="verified"}));
    }
    Ok(
        serde_json::json!({"model_version":"native-gap-recovery/v1","enabled":true,"evidence_bytes":bytes,"coalesced_gap_observations":rejected,"deferred_streams":conn.query_row::<usize,_,_>("SELECT COUNT(*) FROM recovery_debt",[],|r|r.get(0))?,"evicted_terminal_jobs":evicted,"last_evicted_id":last,"jobs":jobs,"limits":config,"store_origin":origin,"global_quality_cleared":false}),
    )
}

fn record_crc(id: u64, sequence: u64, body: &str) -> u32 {
    let mut c = flate2::Crc::new();
    c.update(&id.to_le_bytes());
    c.update(&sequence.to_le_bytes());
    c.update(body.as_bytes());
    c.sum()
}
fn verify_rows(
    conn: &rusqlite::Connection,
    id: u64,
    scope: WindowRequest,
) -> Result<serde_json::Value> {
    let mut accumulator = WindowAccumulator::new(scope)?;
    let mut stmt = conn.prepare(
        "SELECT sequence,body,crc32 FROM recovery_records WHERE job_id=?1 ORDER BY sequence",
    )?;
    for row in stmt.query_map([id], |r| {
        Ok((
            r.get::<_, u64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, u32>(2)?,
        ))
    })? {
        let (sequence, body, checksum) = row?;
        ensure!(
            record_crc(id, sequence, &body) == checksum,
            "recovered trade CRC mismatch"
        );
        let trade: RecoveredTrade = serde_json::from_str(&body)?;
        ensure!(trade.sequence == sequence, "recovery row identity mismatch");
        let native = parse_trade(&trade.native)?;
        ensure!(
            native.sequence == trade.sequence
                && native.ts_ms == trade.ts_ms
                && native.price.to_bits() == trade.price.to_bits()
                && native.qty.to_bits() == trade.qty.to_bits()
                && native.buy == trade.buy,
            "recovery normalization differs from native evidence"
        );
        accumulator.push(trade.window());
    }
    Ok(serde_json::to_value(accumulator.finish())?)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveredTrade {
    sequence: u64,
    ts_ms: u64,
    price: f64,
    qty: f64,
    buy: bool,
    native: serde_json::Value,
}
impl RecoveredTrade {
    fn window(&self) -> WindowTrade {
        WindowTrade {
            sequence: self.sequence,
            ts_ms: self.ts_ms,
            price: self.price,
            qty: self.qty,
            side: if self.buy {
                TradeSide::Buy
            } else {
                TradeSide::Sell
            },
        }
    }
    fn matches(&self, point: &NativeCheckpoint) -> bool {
        self.sequence == point.sequence
            && self.price.to_bits() == point.signature[0]
            && self.qty.to_bits() == point.signature[1]
            && self.ts_ms == point.signature[2]
            && (if self.buy { 1 } else { 2 }) == point.signature[3]
    }
}
#[derive(Debug)]
struct Permanent(&'static str);
impl std::fmt::Display for Permanent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Permanent {}
fn permanent(reason: &'static str) -> anyhow::Error {
    Permanent(reason).into()
}
fn parse_trade(v: &serde_json::Value) -> Result<RecoveredTrade> {
    let price = v["p"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| permanent("invalid_recovery_price"))?;
    let qty = v["q"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| permanent("invalid_recovery_qty"))?;
    if !(price * qty).is_finite() {
        return Err(permanent("invalid_recovery_notional"));
    }
    Ok(RecoveredTrade {
        native: v.clone(),
        sequence: v["a"]
            .as_u64()
            .ok_or_else(|| permanent("missing_recovery_id"))?,
        ts_ms: v["T"]
            .as_u64()
            .filter(|t| *t > 0)
            .ok_or_else(|| permanent("invalid_recovery_timestamp"))?,
        price,
        qty,
        buy: !v["m"]
            .as_bool()
            .ok_or_else(|| permanent("missing_recovery_direction"))?,
    })
}

struct Pager<'a> {
    store: &'a RecoveryStore,
    client: reqwest::Client,
    books: crate::book_integrity::BookRegistry,
    job: &'a Job,
    requests: usize,
    fetched: usize,
    endpoint: Option<String>,
}
impl Pager<'_> {
    async fn page(&mut self, from: u64) -> Result<Vec<RecoveredTrade>> {
        self.requests += 1;
        if self.requests > self.store.config.max_requests_per_attempt {
            return Err(permanent("recovery_request_budget_exceeded"));
        }
        self.books.request_weight(20).await;
        let base = self.endpoint.clone().unwrap_or_else(|| {
            if self.job.gap.scope.market == "spot" {
                "https://api.binance.com/api/v3/aggTrades".into()
            } else {
                "https://fapi.binance.com/fapi/v1/aggTrades".into()
            }
        });
        let mut response = self
            .client
            .get(base)
            .query(&[
                ("symbol", self.job.gap.scope.symbol.clone()),
                ("fromId", from.to_string()),
                ("limit", "1000".into()),
            ])
            .send()
            .await?;
        if matches!(response.status().as_u16(), 418 | 429) {
            let delay = response
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60)
                .clamp(1, 3600);
            self.books.throttle(Duration::from_secs(delay));
            anyhow::bail!("provider_rate_limited");
        }
        response = response.error_for_status()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(permanent("recovery_response_byte_budget_exceeded"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let values: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
        if values.len() > 1000 {
            return Err(permanent("invalid_recovery_page_size"));
        }
        self.fetched += values.len();
        if self.fetched
            > self.store.config.max_rows_per_job + self.store.config.max_requests_per_attempt * 2
        {
            return Err(permanent("recovery_scan_row_budget_exceeded"));
        }
        let rows = values.iter().map(parse_trade).collect::<Result<Vec<_>>>()?;
        if let Some(first) = rows.first()
            && first.sequence != from
        {
            return Err(permanent("provider_from_id_not_honored_or_history_missing"));
        }
        for pair in rows.windows(2) {
            if pair[0].sequence.checked_add(1) != Some(pair[1].sequence)
                || pair[0].ts_ms > pair[1].ts_ms
            {
                return Err(permanent("provider_page_native_gap_or_clock_regression"));
            }
        }
        Ok(rows)
    }
}
async fn recover_job(
    store: &RecoveryStore,
    job: &Job,
    books: crate::book_integrity::BookRegistry,
    endpoint: Option<String>,
) -> Result<()> {
    job.gap.scope.validate()?;
    job.gap.before.validate()?;
    job.gap.after.validate()?;
    if job.gap.scope.end_ms.saturating_sub(job.gap.scope.start_ms) > store.config.max_window_ms {
        return Err(permanent("affected_window_exceeds_configured_span"));
    }
    if !job
        .gap
        .scope
        .symbol
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(permanent("unsupported_recovery_instrument_id"));
    }
    store.clear_staging(job.id).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(store.config.request_timeout_ms))
        .build()?;
    let mut pager = Pager {
        store,
        client,
        books,
        job,
        requests: 0,
        fetched: 0,
        endpoint,
    };
    // Seek backward in native IDs, not wall-clock guesses, until an actual
    // predecessor proves the complete start of the affected time window.
    let mut search = job.gap.before.sequence.saturating_sub(999);
    let first = loop {
        let rows = pager.page(search).await?;
        if let Some(row) = rows.iter().rev().find(|r| r.ts_ms < job.gap.scope.start_ms) {
            break row.sequence;
        }
        if search == 0 {
            return Err(permanent("no_native_before_window_boundary"));
        }
        search = search.saturating_sub(1000);
    };
    let mut cursor = first;
    let mut previous: Option<RecoveredTrade> = None;
    let mut count = 0;
    let mut before_matched = false;
    let mut after_matched = false;
    loop {
        let rows = pager.page(cursor).await?;
        if rows.is_empty() {
            anyhow::bail!("after_window_boundary_not_available");
        }
        let mut staging = Vec::new();
        let mut closed = false;
        for row in rows {
            if let Some(old) = &previous
                && (old.sequence.checked_add(1) != Some(row.sequence) || row.ts_ms < old.ts_ms)
            {
                return Err(permanent("cross_page_native_gap_or_clock_regression"));
            }
            if row.sequence == job.gap.before.sequence {
                if !row.matches(&job.gap.before) {
                    return Err(permanent("before_live_boundary_content_conflict"));
                }
                before_matched = true;
            }
            if row.sequence == job.gap.after.sequence {
                if !row.matches(&job.gap.after) {
                    return Err(permanent("after_live_boundary_content_conflict"));
                }
                after_matched = true;
            }
            count += 1;
            if count > store.config.max_rows_per_job {
                return Err(permanent("recovered_window_row_budget_exceeded"));
            }
            closed = row.ts_ms >= job.gap.scope.end_ms;
            previous = Some(row.clone());
            staging.push(row);
            if closed {
                break;
            }
        }
        store.stage(job.id, staging).await?;
        if closed {
            if !before_matched || !after_matched {
                return Err(permanent("frozen_live_boundaries_missing_from_window"));
            }
            break;
        }
        cursor = previous
            .as_ref()
            .unwrap()
            .sequence
            .checked_add(1)
            .ok_or_else(|| permanent("recovery_native_id_overflow"))?;
    }
    store.verify_and_seal(job).await
}

pub fn spawn_worker(
    store: RecoveryStore,
    metrics: Arc<crate::metrics::AppMetrics>,
    shutdown: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<Result<()>> {
    spawn_worker_endpoint(store, metrics, shutdown, None)
}
pub(crate) fn spawn_worker_endpoint(
    store: RecoveryStore,
    metrics: Arc<crate::metrics::AppMetrics>,
    shutdown: tokio_util::sync::CancellationToken,
    endpoint: Option<String>,
) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        loop {
            if shutdown.is_cancelled() {
                return Ok(());
            }
            let Some(job) = store.claim().await? else {
                tokio::select! {_=shutdown.cancelled()=>return Ok(()),_=tokio::time::sleep(Duration::from_secs(1))=>{},_=store.backend.changed()=>{}}
                continue;
            };
            if now_ms() < job.gap.scope.end_ms {
                store
                    .defer(
                        &job,
                        "await_boundary",
                        "window_not_closed",
                        job.gap.scope.end_ms.saturating_add(1000),
                    )
                    .await?;
                continue;
            }
            let result = tokio::select! {_=shutdown.cancelled()=>return Ok(()),result=recover_job(&store,&job,metrics.books.clone(),endpoint.clone())=>result};
            if let Err(error) = result {
                let permanent = error.downcast_ref::<Permanent>();
                let blocked = permanent.is_some() || job.attempt >= store.config.max_attempts;
                let reason = permanent.map_or(
                    if blocked {
                        "recovery_attempt_budget_exhausted"
                    } else {
                        "provider_or_storage_retry_required"
                    },
                    |e| e.0,
                );
                let delay =
                    (1000u64.saturating_mul(2u64.saturating_pow(job.attempt.min(9)))).min(300000);
                store
                    .defer(
                        &job,
                        if blocked { "blocked" } else { "retry_wait" },
                        reason,
                        now_ms().saturating_add(delay),
                    )
                    .await?;
                tracing::warn!(
                    job_id = job.id,
                    reason,
                    attempt = job.attempt,
                    "native window recovery is not verified"
                );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn directory() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mb-native-repair-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    fn config() -> RecoveryConfig {
        RecoveryConfig {
            max_rows_per_job: 10000,
            max_evidence_bytes: 1024 * 1024,
            max_database_bytes: 8 * 1024 * 1024,
            ..Default::default()
        }
    }
    fn point(id: u64) -> NativeCheckpoint {
        NativeCheckpoint {
            key: "binance:trade:spot:BTCUSDT".into(),
            sequence: id,
            signature: [
                10.0f64.to_bits(),
                1.0f64.to_bits(),
                120000 + id * 20,
                if id % 2 == 1 { 1 } else { 2 },
            ],
        }
    }
    async fn open(root: &std::path::Path, cfg: RecoveryConfig) -> RecoveryStore {
        RecoveryStore::initialize(
            Outbox::open(
                &root.join("recovery.sqlite"),
                "recovery",
                4 * 1024 * 1024,
                cfg.max_database_bytes,
            )
            .unwrap(),
            cfg,
        )
        .await
        .unwrap()
    }
    async fn scheduled(store: &RecoveryStore) -> Job {
        store
            .backend
            .record_checkpoints(vec![point(10), point(13)])
            .await
            .unwrap();
        store.claim().await.unwrap().unwrap()
    }
    fn rows() -> Vec<serde_json::Value> {
        (0..=1502).map(|n|serde_json::json!({"a":n,"p":"10.0","q":"1.0","m":n%2==0,"T":if n==0{119999}else if n==1502{180000}else{120000+n*20}})).collect()
    }
    fn books() -> crate::book_integrity::BookRegistry {
        let r = crate::book_integrity::BookRegistry::default();
        r.configure(crate::book_integrity::BookConfig {
            binance_weight_per_minute: 2400,
            ..Default::default()
        });
        r
    }
    async fn server(
        rows: Vec<serde_json::Value>,
        throttle_first: bool,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let rows = Arc::new(rows);
        let handle = tokio::spawn(async move {
            let mut count = 0;
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                count += 1;
                let mut input = vec![0; 8192];
                let n = socket.read(&mut input).await.unwrap();
                let request = String::from_utf8_lossy(&input[..n]);
                let from = request
                    .split("fromId=")
                    .nth(1)
                    .and_then(|s| s.split(['&', ' ']).next())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                let body = if throttle_first && count == 1 {
                    "{}".into()
                } else {
                    serde_json::to_string(
                        &rows
                            .iter()
                            .filter(|v| v["a"].as_u64().unwrap() >= from)
                            .take(1000)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap()
                };
                let status = if throttle_first && count == 1 {
                    "429 Too Many Requests\r\nRetry-After: 1"
                } else {
                    "200 OK"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (format!("http://{addr}/aggTrades"), handle)
    }
    #[tokio::test]
    async fn actual_rest_pagination_closes_window_and_rechecks_evidence() {
        let root = directory();
        let store = open(&root, config()).await;
        let job = scheduled(&store).await;
        let (endpoint, peer) = server(rows(), false).await;
        assert!(
            store
                .recover_window(job.gap.scope.clone())
                .await
                .unwrap()
                .is_none()
        );
        recover_job(&store, &job, books(), Some(endpoint))
            .await
            .unwrap();
        let report = store
            .recover_window(job.gap.scope.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(report["window"]["recovered"]["trade_count"], 1501);
        assert_eq!(report["window"]["recovered"]["buy_qty"], 751.0);
        assert_eq!(report["window"]["recovered"]["sell_qty"], 750.0);
        assert_eq!(report["global_quality_cleared"], false);
        store
            .backend
            .blocking(|conn| {
                let body: String = conn.query_row(
                    "SELECT body FROM recovery_records WHERE sequence=1",
                    [],
                    |r| r.get(0),
                )?;
                let evidence: serde_json::Value = serde_json::from_str(&body)?;
                assert_eq!(evidence["native"]["p"], "10.0");
                assert_eq!(evidence["native"]["q"], "1.0");
                assert_eq!(evidence["native"]["m"], false);
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            store.backend.status().await.unwrap().pending_records,
            0,
            "compact recovery backend must not archive all events"
        );
        store
            .backend
            .blocking(|conn| {
                conn.execute(
                    "UPDATE recovery_records SET body='{}' WHERE sequence=100",
                    [],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(store.recover_window(job.gap.scope).await.is_err());
        peer.abort();
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn interrupted_job_survives_reopen_and_partial_rows_never_qualify() {
        let root = directory();
        let cfg = config();
        let store = open(&root, cfg.clone()).await;
        let job = scheduled(&store).await;
        store
            .stage(
                job.id,
                vec![RecoveredTrade {
                    sequence: 0,
                    ts_ms: 119999,
                    price: 10.0,
                    qty: 1.0,
                    buy: false,
                    native: rows()[0].clone(),
                }],
            )
            .await
            .unwrap();
        assert!(
            store
                .recover_window(job.gap.scope.clone())
                .await
                .unwrap()
                .is_none()
        );
        drop(store);
        let store = open(&root, cfg).await;
        let resumed = store.claim().await.unwrap().unwrap();
        assert_eq!(resumed.id, job.id);
        assert_eq!(resumed.attempt, 2);
        assert_eq!(
            store.backend.native_checkpoints().await.unwrap()[0].sequence,
            13
        );
        let (endpoint, peer) = server(rows(), false).await;
        recover_job(&store, &resumed, books(), Some(endpoint))
            .await
            .unwrap();
        assert!(store.recover_window(job.gap.scope).await.unwrap().is_some());
        peer.abort();
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn provider_id_gap_or_live_boundary_conflict_cannot_seal() {
        for conflict in [false, true] {
            let root = directory();
            let store = open(&root, config()).await;
            let job = scheduled(&store).await;
            let mut data = rows();
            if conflict {
                data[13]["q"] = "2.0".into();
            } else {
                data.remove(12);
            }
            let (endpoint, peer) = server(data, false).await;
            let error = recover_job(&store, &job, books(), Some(endpoint))
                .await
                .unwrap_err();
            assert!(error.downcast_ref::<Permanent>().is_some());
            assert!(store.recover_window(job.gap.scope).await.unwrap().is_none());
            peer.abort();
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn throttling_is_retryable_and_rate_budget_is_shared() {
        let root = directory();
        let store = open(&root, config()).await;
        let job = scheduled(&store).await;
        let registry = books();
        let (endpoint, peer) = server(rows(), true).await;
        let error = recover_job(&store, &job, registry.clone(), Some(endpoint.clone()))
            .await
            .unwrap_err();
        assert!(error.downcast_ref::<Permanent>().is_none());
        assert!(
            store
                .recover_window(job.gap.scope.clone())
                .await
                .unwrap()
                .is_none()
        );
        let before = std::time::Instant::now();
        recover_job(&store, &job, registry, Some(endpoint))
            .await
            .unwrap();
        assert!(before.elapsed() >= Duration::from_millis(900));
        assert!(store.recover_window(job.gap.scope).await.unwrap().is_some());
        peer.abort();
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn full_job_queue_preserves_coalesced_debt_and_promotes_after_terminal_job() {
        let root = directory();
        let store = open(
            &root,
            RecoveryConfig {
                max_jobs: 1,
                ..config()
            },
        )
        .await;
        let job = scheduled(&store).await;
        for id in [15, 18] {
            store
                .backend
                .record_checkpoints(vec![point(id)])
                .await
                .unwrap();
        }
        let status = store.status().await.unwrap();
        assert_eq!(status["deferred_streams"], 1);
        assert_eq!(status["coalesced_gap_observations"], 2);
        store
            .defer(&job, "blocked", "fixture_terminal", now_ms())
            .await
            .unwrap();
        let next = store.claim().await.unwrap().unwrap();
        assert_eq!(next.gap.before.sequence, 13);
        assert_eq!(next.gap.after.sequence, 18);
        assert_eq!(store.status().await.unwrap()["deferred_streams"], 0);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn gap_and_checkpoint_commit_atomically_with_event_and_rollback() {
        let root = directory();
        let store = open(
            &root,
            RecoveryConfig {
                max_jobs: 1,
                max_deferred_streams: 1,
                ..config()
            },
        )
        .await;
        let events = |id: u64| {
            let p = point(id);
            (
                Arc::new(crate::types::DataEvent::Trade(crate::types::TradeTick {
                    exchange: "binance",
                    market: crate::types::MarketKind::Spot,
                    symbol: "BTCUSDT".into(),
                    price: 10.0,
                    qty: 1.0,
                    side: if id % 2 == 1 {
                        TradeSide::Buy
                    } else {
                        TradeSide::Sell
                    },
                    trade_id: Some(id.to_string().into()),
                    ts_ms: p.signature[2],
                })),
                now_ms(),
            )
        };
        store
            .backend
            .append(vec![events(10), events(13)])
            .await
            .unwrap();
        assert_eq!(
            store.status().await.unwrap()["jobs"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(store.backend.status().await.unwrap().pending_records, 2);
        let mut b = point(10);
        b.key = "binance:trade:spot:B".into();
        let mut c = b.clone();
        c.key = "binance:trade:spot:C".into();
        store
            .backend
            .record_checkpoints(vec![b.clone(), c.clone()])
            .await
            .unwrap();
        b.sequence = 13;
        store.backend.record_checkpoints(vec![b]).await.unwrap();
        c.sequence = 13;
        assert!(store.backend.record_checkpoints(vec![c]).await.is_err());
        let points = store.backend.native_checkpoints().await.unwrap();
        assert_eq!(
            points
                .iter()
                .find(|p| p.key.ends_with(":C"))
                .unwrap()
                .sequence,
            10,
            "failed gap schedule must not advance checkpoint"
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn worker_retries_transient_errors_without_clearing_source_quality() {
        let root = directory();
        let store = open(&root, config()).await;
        store
            .backend
            .record_checkpoints(vec![point(10), point(13)])
            .await
            .unwrap();
        let metrics = crate::metrics::AppMetrics::new();
        metrics.books.configure(crate::book_integrity::BookConfig {
            binance_weight_per_minute: 2400,
            ..Default::default()
        });
        metrics.ingestion.loss("fixture_gap", 2);
        let (endpoint, peer) = server(rows(), true).await;
        let stop = tokio_util::sync::CancellationToken::new();
        let task =
            spawn_worker_endpoint(store.clone(), metrics.clone(), stop.clone(), Some(endpoint));
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let status = store.status().await.unwrap();
                if status["jobs"][0]["state"] == "verified" {
                    assert!(status["jobs"][0]["attempt"].as_u64().unwrap() >= 2);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        assert!(metrics.ingestion.integrity().has_known_loss);
        stop.cancel();
        task.await.unwrap().unwrap();
        peer.abort();
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn open_window_waits_for_real_after_boundary_without_fabricated_zero() {
        let root = directory();
        let store = open(&root, config()).await;
        let mut before = point(10);
        before.signature[2] = now_ms() + 1000;
        let mut after = before.clone();
        after.sequence = 13;
        store
            .backend
            .record_checkpoints(vec![before, after])
            .await
            .unwrap();
        let metrics = crate::metrics::AppMetrics::new();
        let stop = tokio_util::sync::CancellationToken::new();
        let task = spawn_worker_endpoint(
            store.clone(),
            metrics,
            stop.clone(),
            Some("http://127.0.0.1:9/unreachable".into()),
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let status = store.status().await.unwrap();
                if status["jobs"][0]["state"] == "await_boundary" {
                    assert!(status["jobs"][0]["next_attempt_ms"].as_u64().unwrap() > now_ms());
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        stop.cancel();
        task.await.unwrap().unwrap();
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
