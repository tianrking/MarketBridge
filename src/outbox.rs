//! Durable, bounded pending delivery. Commit precedes publication; ACK deletion
//! follows the remote idempotent write. Export/replay preserve origin+sequence.
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::types::DataEvent;

const MAX_RECORD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventIdentity {
    pub origin: std::borrow::Cow<'static, str>,
    pub sequence: u64,
}

impl EventIdentity {
    pub fn process_origin() -> &'static str {
        static ORIGIN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        ORIGIN.get_or_init(|| {
            Connection::open_in_memory()
                .and_then(|conn| {
                    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
                })
                .expect("initialize process event identity")
        })
    }

    pub fn local() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self {
            origin: std::borrow::Cow::Borrowed(Self::process_origin()),
            sequence: NEXT
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .checked_add(1)
                .expect("event identity sequence exhausted"),
        }
    }

    pub fn id(&self) -> String {
        format!("{}:{}", self.origin, self.sequence)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingEvent {
    pub identity: EventIdentity,
    pub stream: String,
    pub received_at_ms: u64,
    pub payload: String,
    pub crc32: u32,
}

impl PendingEvent {
    pub fn physical_stream(&self) -> String {
        format!("{}:origin:{}", self.stream, self.identity.origin)
    }
    fn bytes(&self) -> u64 {
        (self.payload.len() + self.stream.len() + 128) as u64
    }
    fn checksum(&self) -> u32 {
        let data = serde_json::to_vec(&(
            &self.identity,
            &self.stream,
            self.received_at_ms,
            &self.payload,
        ))
        .expect("owned outbox record serializes");
        let mut crc = flate2::Crc::new();
        crc.update(&data);
        crc.sum()
    }
    fn validate(&self, origin: &str) -> Result<()> {
        ensure!(
            self.identity.origin == origin
                && self.identity.sequence > 0
                && self.identity.sequence <= i64::MAX as u64,
            "outbox identity mismatch"
        );
        ensure!(
            !self.stream.is_empty()
                && self.stream.len() <= 512
                && self.payload.len() <= MAX_RECORD_BYTES,
            "outbox record exceeds limits"
        );
        ensure!(
            self.received_at_ms <= i64::MAX as u64,
            "outbox observation time exceeds SQLite integer range"
        );
        ensure!(self.crc32 == self.checksum(), "outbox record CRC mismatch");
        let value: serde_json::Value = serde_json::from_str(&self.payload)?;
        ensure!(
            value.is_object() && value["type"].is_string(),
            "outbox payload must be a normalized event object"
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct Outbox {
    connection: Arc<Mutex<Connection>>,
    gate: Arc<Semaphore>,
    origin: String,
    prefix: String,
    max_pending_bytes: u64,
    _lease: Arc<Mutex<Connection>>,
    delivery: Arc<Mutex<DeliveryObservation>>,
    changed: Arc<tokio::sync::Notify>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeliveryObservation {
    pub configured: bool,
    pub verified: bool,
    pub require_local_aof: bool,
    pub last_verified_at_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OutboxStatus {
    pub origin: String,
    pub oldest_pending_received_at_ms: Option<u64>,
    pub pending_records: u64,
    pub pending_bytes: u64,
    pub max_pending_bytes: u64,
    pub last_assigned_sequence: u64,
    pub delivery: DeliveryObservation,
}

impl Outbox {
    pub fn open(
        path: &Path,
        prefix: &str,
        max_pending_bytes: u64,
        max_database_bytes: u64,
    ) -> Result<Self> {
        ensure!(
            !prefix.is_empty() && prefix.len() <= 128,
            "outbox stream prefix must be 1..128 bytes"
        );
        ensure!(
            max_pending_bytes > 0 && max_database_bytes >= 4 * 1024 * 1024,
            "invalid outbox disk budgets"
        );
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".lease.sqlite");
        let lease = Connection::open(std::path::PathBuf::from(lock_path))?;
        lease.busy_timeout(std::time::Duration::ZERO)?;
        lease.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE IF NOT EXISTS lease(id INTEGER PRIMARY KEY); BEGIN EXCLUSIVE;")
            .context("outbox already owned by another process; offline replay requires stopping that instance")?;
        let conn = Connection::open(path).context("open durable outbox")?;
        conn.busy_timeout(std::time::Duration::from_secs(3))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=256; PRAGMA journal_size_limit=4194304;")?;
        let page_size: u64 = conn.pragma_query_value(None, "page_size", |r| r.get(0))?;
        let pages = max_database_bytes / page_size;
        let actual: u64 =
            conn.query_row(&format!("PRAGMA max_page_count={pages}"), [], |r| r.get(0))?;
        ensure!(
            actual <= pages,
            "existing outbox database exceeds configured disk budget"
        );
        conn.execute_batch("CREATE TABLE IF NOT EXISTS outbox_state (id INTEGER PRIMARY KEY CHECK(id=1), origin TEXT NOT NULL, prefix TEXT NOT NULL, pending_bytes INTEGER NOT NULL, pending_records INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS outbox_events (sequence INTEGER PRIMARY KEY AUTOINCREMENT, stream TEXT NOT NULL, received_at_ms INTEGER NOT NULL, payload TEXT NOT NULL, crc32 INTEGER NOT NULL);")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS source_checkpoints (stream TEXT PRIMARY KEY, checkpoint TEXT NOT NULL, crc32 INTEGER NOT NULL);")?;
        conn.execute(
            "INSERT OR IGNORE INTO outbox_state VALUES(1,lower(hex(randomblob(16))),?1,0,0)",
            [prefix],
        )?;
        let (origin, saved_prefix, saved_bytes): (String, String, u64) = conn.query_row(
            "SELECT origin,prefix,pending_bytes FROM outbox_state WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        ensure!(
            origin.len() == 32 && origin.bytes().all(|b| b.is_ascii_hexdigit()),
            "outbox origin is corrupt"
        );
        ensure!(
            prefix == saved_prefix,
            "outbox prefix changed; use a separate database for a new delivery namespace"
        );
        let counted: u64 = conn.query_row("SELECT COALESCE(SUM(length(CAST(payload AS BLOB))+length(CAST(stream AS BLOB))+128),0) FROM outbox_events", [], |r| r.get(0))?;
        ensure!(
            saved_bytes == counted && counted <= max_pending_bytes,
            "outbox pending accounting corrupt or above budget"
        );
        let saved_count: u64 = conn.query_row(
            "SELECT pending_records FROM outbox_state WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let real_count: u64 =
            conn.query_row("SELECT COUNT(*) FROM outbox_events", [], |r| r.get(0))?;
        ensure!(
            saved_count == real_count,
            "outbox pending record count corrupt"
        );
        Ok(Self {
            connection: Arc::new(Mutex::new(conn)),
            gate: Arc::new(Semaphore::new(1)),
            origin,
            prefix: prefix.into(),
            max_pending_bytes,
            _lease: Arc::new(Mutex::new(lease)),
            delivery: Arc::new(Mutex::new(DeliveryObservation::default())),
            changed: Arc::new(tokio::sync::Notify::new()),
        })
    }

    pub async fn changed(&self) {
        self.changed.notified().await;
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn configure_delivery(&self, require_local_aof: bool) {
        let mut state = self.delivery.lock().unwrap_or_else(|e| e.into_inner());
        state.configured = true;
        state.require_local_aof = require_local_aof;
    }
    pub fn delivery_result(&self, error: Option<&str>) {
        let mut state = self.delivery.lock().unwrap_or_else(|e| e.into_inner());
        state.verified = error.is_none();
        state.last_error = error.map(|e| e.chars().take(512).collect());
        if error.is_none() {
            state.last_verified_at_ms = Some(crate::types::now_ms());
        }
    }

    async fn blocking<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = self.gate.clone().acquire_owned().await?;
        let conn = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut guard = conn
                .lock()
                .map_err(|_| anyhow::anyhow!("outbox mutex poisoned"))?;
            action(&mut guard)
        })
        .await?
    }

    pub async fn native_checkpoints(&self) -> Result<Vec<crate::ingestion::NativeCheckpoint>> {
        self.blocking(|conn| {
            let mut stmt = conn.prepare(
                "SELECT stream,checkpoint,crc32 FROM source_checkpoints ORDER BY stream",
            )?;
            let texts = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u32>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            texts
                .into_iter()
                .map(|(key, text, crc)| {
                    ensure!(
                        checkpoint_crc(&text) == crc,
                        "native checkpoint CRC mismatch"
                    );
                    let point: crate::ingestion::NativeCheckpoint = serde_json::from_str(&text)?;
                    ensure!(key == point.key, "native checkpoint key mismatch");
                    point.validate()?;
                    Ok(point)
                })
                .collect()
        })
        .await
    }

    #[cfg(test)]
    pub async fn append(&self, events: Vec<(Arc<DataEvent>, u64)>) -> Result<Vec<EventIdentity>> {
        self.append_traced(
            events
                .into_iter()
                .map(|(event, wall)| (event, wall, None))
                .collect(),
        )
        .await
    }

    pub async fn append_traced(
        &self,
        events: Vec<(
            Arc<DataEvent>,
            u64,
            Option<crate::ingestion::SourceIntegrity>,
        )>,
    ) -> Result<Vec<EventIdentity>> {
        ensure!(
            !events.is_empty() && events.len() <= 64,
            "outbox transaction requires 1..64 events"
        );
        let origin = self.origin.clone();
        let prefix = self.prefix.clone();
        let budget = self.max_pending_bytes;
        let outcome = self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let mut bytes: u64 = tx.query_row("SELECT pending_bytes FROM outbox_state WHERE id=1", [], |r| r.get(0))?;
            let mut identities = Vec::with_capacity(events.len());
            for (event, received, source_quality) in events {
                ensure!(received <= i64::MAX as u64, "outbox observation time out of range");
                #[derive(Serialize)]
                struct Payload<'a> { #[serde(flatten)] event:&'a DataEvent, #[serde(skip_serializing_if="Option::is_none")] source_quality:Option<crate::ingestion::SourceIntegrity> }
                let payload = serde_json::to_string(&Payload {event:&event,source_quality})?;
                if let Some(point)=crate::ingestion::native_checkpoint(&event) {
                    let previous:Option<(String,u32)>=tx.query_row("SELECT checkpoint,crc32 FROM source_checkpoints WHERE stream=?1",[&point.key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                    if let Some((previous,crc))=previous {
                        ensure!(checkpoint_crc(&previous)==crc,"native checkpoint CRC mismatch");
                        let old:crate::ingestion::NativeCheckpoint=serde_json::from_str(&previous)?;
                        ensure!(point.sequence>old.sequence,"native checkpoint regressed; admission must quarantine this event");
                    } else {
                        let count:usize=tx.query_row("SELECT COUNT(*) FROM source_checkpoints",[],|r|r.get(0))?;
                        ensure!(count<65536,"persisted native checkpoint budget exhausted");
                    }
                    let checkpoint=serde_json::to_string(&point)?;
                    tx.execute("INSERT INTO source_checkpoints(stream,checkpoint,crc32) VALUES(?1,?2,?3) ON CONFLICT(stream) DO UPDATE SET checkpoint=excluded.checkpoint,crc32=excluded.crc32",
                        params![point.key,checkpoint,checkpoint_crc(&checkpoint)])?;
                }
                let stream = crate::redis_sink::redis_stream_name(&prefix, &event);
                ensure!(payload.len() <= MAX_RECORD_BYTES && stream.len() <= 512, "outbox event exceeds record budget");
                let cost = (payload.len() + stream.len() + 128) as u64;
                bytes = bytes.checked_add(cost).context("outbox byte accounting overflow")?;
                ensure!(bytes <= budget, "outbox pending byte budget exhausted; no events in this batch were committed");
                tx.execute("INSERT INTO outbox_events(stream,received_at_ms,payload,crc32) VALUES(?1,?2,?3,0)", params![stream, received, payload])?;
                let identity = EventIdentity { origin:origin.clone().into(), sequence:tx.last_insert_rowid() as u64 };
                let row = PendingEvent { identity:identity.clone(),stream,received_at_ms:received,payload,crc32:0 };
                tx.execute("UPDATE outbox_events SET crc32=?1 WHERE sequence=?2", params![row.checksum(),identity.sequence])?;
                identities.push(identity);
            }
            tx.execute("UPDATE outbox_state SET pending_bytes=?1,pending_records=pending_records+?2 WHERE id=1", params![bytes,identities.len()])?;
            tx.commit()?;
            Ok(identities)
        }).await;
        if outcome.is_ok() {
            self.changed.notify_one();
        }
        outcome
    }

    pub async fn pending(&self, limit: usize) -> Result<Vec<PendingEvent>> {
        ensure!(
            (1..=4096).contains(&limit),
            "outbox read limit must be 1..4096"
        );
        let origin = self.origin.clone();
        self.blocking(move |conn| read_rows(conn, &origin, limit))
            .await
    }

    pub async fn acknowledge(&self, identities: Vec<EventIdentity>) -> Result<()> {
        ensure!(
            identities.len() <= 4096 && identities.iter().all(|id| id.origin == self.origin),
            "outbox ACK identity mismatch"
        );
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let mut bytes: u64 = tx.query_row("SELECT pending_bytes FROM outbox_state WHERE id=1", [], |r| r.get(0))?;
            let mut removed = 0usize;
            for id in identities {
                let cost: Option<u64> = tx.query_row("SELECT length(CAST(payload AS BLOB))+length(CAST(stream AS BLOB))+128 FROM outbox_events WHERE sequence=?1", [id.sequence], |r| r.get(0)).optional()?;
                if let Some(cost) = cost {
                    tx.execute("DELETE FROM outbox_events WHERE sequence=?1", [id.sequence])?;
                    bytes = bytes.checked_sub(cost).context("outbox ACK accounting underflow")?;
                    removed += 1;
                }
            }
            tx.execute("UPDATE outbox_state SET pending_bytes=?1,pending_records=pending_records-?2 WHERE id=1", params![bytes,removed])?;
            tx.commit()?; Ok(())
        }).await
    }

    pub async fn status(&self) -> Result<OutboxStatus> {
        let origin = self.origin.clone();
        let budget = self.max_pending_bytes;
        let delivery = self
            .delivery
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        self.blocking(move |conn| {
            Ok(OutboxStatus {
                origin,
                delivery,
                pending_records: conn.query_row(
                    "SELECT pending_records FROM outbox_state WHERE id=1",
                    [],
                    |r| r.get(0),
                )?,
                pending_bytes: conn.query_row(
                    "SELECT pending_bytes FROM outbox_state WHERE id=1",
                    [],
                    |r| r.get(0),
                )?,
                oldest_pending_received_at_ms: conn
                    .query_row(
                        "SELECT received_at_ms FROM outbox_events ORDER BY sequence LIMIT 1",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?,
                max_pending_bytes: budget,
                last_assigned_sequence: conn
                    .query_row(
                        "SELECT seq FROM sqlite_sequence WHERE name='outbox_events'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(0),
            })
        })
        .await
    }

    pub async fn export(&self, path: &Path) -> Result<u64> {
        let path = path.to_path_buf();
        let origin = self.origin.clone();
        self.blocking(move |conn| {
            let mut writer = BufWriter::new(OpenOptions::new().create_new(true).write(true).open(path)?);
            let last_sequence:u64=conn.query_row("SELECT seq FROM sqlite_sequence WHERE name='outbox_events'",[],|r|r.get(0)).optional()?.unwrap_or(0);
            let header=serde_json::to_vec(&serde_json::json!({"format":"outbox-export/v1","origin":origin,"last_assigned_sequence":last_sequence}))?;
            let mut crc=flate2::Crc::new();crc.update(&header);crc.update(b"\n");
            writer.write_all(&header)?;writer.write_all(b"\n")?;
            let mut stmt = conn.prepare("SELECT sequence,stream,received_at_ms,payload,crc32 FROM outbox_events ORDER BY sequence")?;
            let mut rows = stmt.query([])?; let mut count = 0u64;
            while let Some(row) = rows.next()? {
                let item = decode_row(row, &origin)?; item.validate(&origin)?;
                let line = serde_json::to_vec(&item)?; crc.update(&line); crc.update(b"\n");
                writer.write_all(&line)?; writer.write_all(b"\n")?; count += 1;
            }
            writeln!(writer,"{}",serde_json::json!({"end":true,"records":count,"crc32":crc.sum()}))?;
            writer.flush()?; writer.get_ref().sync_all()?; Ok(count)
        }).await
    }

    pub async fn replay(&self, path: &Path) -> Result<u64> {
        let path = path.to_path_buf();
        let origin = self.origin.clone();
        let prefix = format!("{}:", self.prefix);
        let budget = self.max_pending_bytes;
        let outcome = self.blocking(move |conn| {
            let mut reader = BufReader::new(File::open(path)?);
            let header_line=bounded_line(&mut reader)?;
            let header: serde_json::Value = serde_json::from_slice(&header_line)?;
            ensure!(header["format"]=="outbox-export/v1" && header["origin"]==origin, "outbox export header/origin mismatch");
            let last_sequence=header["last_assigned_sequence"].as_u64().filter(|n|*n<=i64::MAX as u64).context("outbox export sequence header invalid")?;
            let tx = conn.transaction()?;
            let mut bytes: u64 = tx.query_row("SELECT pending_bytes FROM outbox_state WHERE id=1", [], |r|r.get(0))?;
            let mut crc = flate2::Crc::new();crc.update(&header_line); let mut count=0u64; let mut added=0u64; let mut previous=0;
            loop {
                let line = bounded_line(&mut reader)?;
                let value: serde_json::Value = serde_json::from_slice(&line)?;
                if value["end"] == true {
                    ensure!(value["records"].as_u64()==Some(count) && value["crc32"].as_u64()==Some(crc.sum() as u64), "outbox export footer mismatch");
                    ensure!(bounded_line(&mut reader)?.is_empty(), "outbox export has trailing data");
                    break;
                }
                let row: PendingEvent = serde_json::from_value(value)?;
                row.validate(&origin)?;
                ensure!(row.stream.starts_with(&prefix) && row.identity.sequence > previous && row.identity.sequence <= last_sequence, "outbox replay namespace/order mismatch");
                previous = row.identity.sequence; crc.update(&line); count += 1;
                let old = tx.query_row("SELECT sequence,stream,received_at_ms,payload,crc32 FROM outbox_events WHERE sequence=?1", [row.identity.sequence], |r|decode_row(r,&origin)).optional()?;
                if let Some(old) = old { ensure!(old==row,"outbox replay identity collision"); }
                else {
                    bytes = bytes.checked_add(row.bytes()).context("outbox replay accounting overflow")?;
                    ensure!(bytes <= budget, "outbox replay exceeds pending byte budget");
                    tx.execute("INSERT INTO outbox_events(sequence,stream,received_at_ms,payload,crc32) VALUES(?1,?2,?3,?4,?5)", params![row.identity.sequence,row.stream,row.received_at_ms,row.payload,row.crc32])?; added += 1;
                }
            }
            // Even an empty export carries the origin's assigned high water
            // mark; replay must never make an acknowledged identity reusable.
            tx.execute("INSERT INTO sqlite_sequence(name,seq) SELECT 'outbox_events',?1 WHERE NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='outbox_events')",[last_sequence])?;
            tx.execute("UPDATE sqlite_sequence SET seq=MAX(seq,?1) WHERE name='outbox_events'",[last_sequence])?;
            tx.execute("UPDATE outbox_state SET pending_bytes=?1,pending_records=pending_records+?2 WHERE id=1", params![bytes,added])?; tx.commit()?; Ok(added)
        }).await;
        if outcome.is_ok() {
            self.changed.notify_one();
        }
        outcome
    }
}

fn checkpoint_crc(text: &str) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(text.as_bytes());
    crc.sum()
}

fn bounded_line(reader: &mut BufReader<File>) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    reader
        .take((MAX_RECORD_BYTES * 2 + 4096) as u64)
        .read_until(b'\n', &mut line)?;
    ensure!(
        line.len() < MAX_RECORD_BYTES * 2 + 4096,
        "outbox export line exceeds budget"
    );
    Ok(line)
}

fn decode_row(row: &rusqlite::Row<'_>, origin: &str) -> rusqlite::Result<PendingEvent> {
    Ok(PendingEvent {
        identity: EventIdentity {
            origin: std::borrow::Cow::Owned(origin.to_owned()),
            sequence: row.get(0)?,
        },
        stream: row.get(1)?,
        received_at_ms: row.get(2)?,
        payload: row.get(3)?,
        crc32: row.get(4)?,
    })
}

fn read_rows(conn: &Connection, origin: &str, limit: usize) -> Result<Vec<PendingEvent>> {
    let mut stmt = conn.prepare("SELECT sequence,stream,received_at_ms,payload,crc32 FROM outbox_events ORDER BY sequence LIMIT ?1")?;
    let mut rows = Vec::new();
    let mut bytes = 0u64;
    for row in stmt.query_map([limit], |r| decode_row(r, origin))? {
        let row = row?;
        row.validate(origin)?;
        if !rows.is_empty() && bytes + row.bytes() > 4 * 1024 * 1024 {
            break;
        }
        bytes += row.bytes();
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FundingRateTick, now_ms};
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "marketbridge-outbox-{}-{}-{}",
            std::process::id(),
            now_ms(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    fn event(n: u64) -> (Arc<DataEvent>, u64) {
        (
            Arc::new(DataEvent::FundingRate(FundingRateTick {
                exchange: "test",
                symbol: "BTCUSDT".into(),
                funding_rate: 0.01,
                next_funding_time_ms: None,
                mark_price: None,
                index_price: None,
                ts_ms: n,
            })),
            n,
        )
    }
    fn open(root: &Path, budget: u64) -> Outbox {
        Outbox::open(
            &root.join("events.sqlite"),
            "ticks",
            budget,
            4 * 1024 * 1024,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn durable_reads_bound_large_payload_batches_by_bytes() {
        let path = root();
        let box_ = Outbox::open(
            &path.join("outbox.sqlite"),
            "fixture",
            32 * 1024 * 1024,
            64 * 1024 * 1024,
        )
        .unwrap();
        let event = Arc::new(crate::types::DataEvent::OrderBook(
            crate::types::OrderBookTick {
                exchange: "fixture",
                market: crate::types::MarketKind::Perp,
                symbol: "X".into(),
                bids: vec![
                    crate::types::BookLevel {
                        price: 1234.0,
                        qty: 10.0
                    };
                    2000
                ],
                asks: vec![],
                last_update_id: None,
                ts_ms: 1,
            },
        ));
        for _ in 0..3 {
            box_.append((0..64).map(|_| (event.clone(), 1)).collect())
                .await
                .unwrap();
        }
        let rows = box_.pending(4096).await.unwrap();
        assert!(!rows.is_empty() && rows.len() < 192);
        assert!(rows.iter().map(PendingEvent::bytes).sum::<u64>() <= 4 * 1024 * 1024);
        assert_eq!(
            box_.status().await.unwrap().oldest_pending_received_at_ms,
            Some(1)
        );
        drop(box_);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[tokio::test]
    async fn restart_preserves_identity_and_acked_sequences_are_never_reused() {
        let root = root();
        let box_ = open(&root, 1024 * 1024);
        let first = box_.append(vec![event(1), event(2)]).await.unwrap();
        assert_eq!(box_.pending(10).await.unwrap()[0].identity, first[0]);
        assert!(
            Outbox::open(
                &root.join("events.sqlite"),
                "ticks",
                1024 * 1024,
                4 * 1024 * 1024
            )
            .is_err(),
            "a second process must not own delivery"
        );
        box_.acknowledge(first.clone()).await.unwrap();
        drop(box_);
        let box_ = open(&root, 1024 * 1024);
        let next = box_.append(vec![event(3)]).await.unwrap();
        assert_eq!(next[0].origin, first[0].origin);
        assert!(next[0].sequence > first[1].sequence);
        assert_eq!(box_.status().await.unwrap().pending_records, 1);
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn pending_quota_rolls_back_entire_batch_and_keeps_existing_rows() {
        let root = root();
        let box_ = open(&root, 600);
        box_.append(vec![event(1)]).await.unwrap();
        let before = box_.pending(10).await.unwrap();
        assert!(box_.append(vec![event(2), event(3)]).await.is_err());
        assert_eq!(box_.pending(10).await.unwrap(), before);
        let status = box_.status().await.unwrap();
        assert_eq!(status.pending_records, 1);
        assert_eq!(status.pending_bytes, before[0].bytes());
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn checked_replay_is_idempotent_and_truncation_rolls_back() {
        let root = root();
        let box_ = open(&root, 1024 * 1024);
        let file = root.join("export.jsonl");
        let ids = box_.append(vec![event(1), event(2)]).await.unwrap();
        assert_eq!(box_.export(&file).await.unwrap(), 2);
        assert!(
            box_.export(&file).await.is_err(),
            "an existing export must not be overwritten"
        );
        assert_eq!(box_.replay(&file).await.unwrap(), 0);
        box_.acknowledge(ids).await.unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        let mut lines = content.lines().collect::<Vec<_>>();
        lines.pop();
        let content = lines.join("\n") + "\n";
        let truncated = root.join("truncated.jsonl");
        std::fs::write(&truncated, content).unwrap();
        assert!(box_.replay(&truncated).await.is_err());
        assert_eq!(box_.status().await.unwrap().pending_records, 0);
        assert_eq!(box_.replay(&file).await.unwrap(), 2);
        assert_eq!(box_.status().await.unwrap().pending_records, 2);
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn corrupt_payload_and_cross_origin_replay_are_rejected() {
        let root = root();
        let box_ = open(&root, 1024 * 1024);
        let file = root.join("export.jsonl");
        box_.append(vec![event(1)]).await.unwrap();
        box_.export(&file).await.unwrap();
        let other_root = root.join("other");
        let other = open(&other_root, 1024 * 1024);
        assert!(other.replay(&file).await.is_err());
        assert_eq!(other.status().await.unwrap().pending_records, 0);
        box_.connection
            .lock()
            .unwrap()
            .execute("UPDATE outbox_events SET payload='{}'", [])
            .unwrap();
        assert!(box_.pending(10).await.is_err());
        drop(other);
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn sqlite_page_budget_failure_is_atomic() {
        use crate::types::ExternalSignalTick;
        let root = root();
        let box_ = open(&root, 32 * 1024 * 1024);
        box_.append(vec![event(1)]).await.unwrap();
        let before = box_.status().await.unwrap().pending_bytes;
        let rows = (0..32)
            .map(|n| {
                (
                    Arc::new(DataEvent::ExternalSignal(ExternalSignalTick {
                        source: "test",
                        source_instance: None,
                        source_time_ms: None,
                        category: "fixture".into(),
                        symbol: None,
                        metric: "value".into(),
                        value: None,
                        score: None,
                        title: None,
                        url: None,
                        ts_ms: n,
                        raw: Some(serde_json::Value::String("x".repeat(200_000))),
                    })),
                    n,
                )
            })
            .collect();
        assert!(box_.append(rows).await.is_err());
        assert_eq!(box_.status().await.unwrap().pending_records, 1);
        assert_eq!(box_.status().await.unwrap().pending_bytes, before);
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn native_checkpoint_survives_ack_deletion_and_detects_corruption() {
        let root = root();
        let box_ = open(&root, 1024 * 1024);
        let event = Arc::new(DataEvent::Trade(crate::types::TradeTick {
            exchange: "binance",
            market: crate::types::MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 1.0,
            side: crate::types::TradeSide::Buy,
            trade_id: Some("42".into()),
            ts_ms: now_ms(),
        }));
        let identities = box_.append(vec![(event, now_ms())]).await.unwrap();
        box_.acknowledge(identities).await.unwrap();
        drop(box_);
        let box_ = open(&root, 1024 * 1024);
        let points = box_.native_checkpoints().await.unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].sequence, 42);
        box_.connection
            .lock()
            .unwrap()
            .execute("UPDATE source_checkpoints SET crc32=0", [])
            .unwrap();
        assert!(box_.native_checkpoints().await.is_err());
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }
}
