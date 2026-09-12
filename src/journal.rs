//! Opt-in bounded normalized-event journal. CRC detects corruption, not tampering.
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    metrics::AppMetrics,
    types::{DataEvent, now_ms},
};

const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_LINE: usize = 1024 * 1024;
static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct JournalMessage {
    pub event: Arc<DataEvent>,
    pub received_at_ms: u64,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Record {
    version: String,
    pub(crate) sequence: u64,
    pub(crate) received_at_ms: u64,
    dropped_total: u64,
    pub(crate) payload: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct CheckedRecord {
    body: String,
    crc32: u32,
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

struct Journal {
    writer: BufWriter<File>,
    partial: PathBuf,
    sequence: u64,
    bytes: u64,
}

impl Journal {
    fn create(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let seq = SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let partial = root.join(format!(
            "session-{}-{}-{seq}.partial",
            now_ms(),
            std::process::id()
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        Ok(Self {
            writer: BufWriter::new(file),
            partial,
            sequence: 0,
            bytes: 0,
        })
    }

    fn append(
        &mut self,
        received_at_ms: u64,
        dropped_total: u64,
        payload: serde_json::Value,
    ) -> Result<()> {
        let body = serde_json::to_string(&Record {
            version: "journal/v1".into(),
            sequence: self.sequence + 1,
            received_at_ms,
            dropped_total,
            payload,
        })?;
        let line = serde_json::to_vec(&CheckedRecord {
            crc32: checksum(body.as_bytes()),
            body,
        })?;
        ensure!(line.len() < MAX_LINE, "journal record exceeds 1 MiB");
        ensure!(
            self.bytes + (line.len() as u64) < MAX_BYTES,
            "journal session reached 256 MiB cap; start a new session"
        );
        self.writer.write_all(&line)?;
        self.writer.write_all(b"\n")?;
        self.bytes += line.len() as u64 + 1;
        self.sequence += 1;
        // Prefix is visible; sync durability is guaranteed only at seal.
        self.writer.flush()?;
        Ok(())
    }

    fn seal(mut self, dropped_total: u64) -> Result<PathBuf> {
        self.append(
            now_ms(),
            dropped_total,
            serde_json::json!({"type":"session_end"}),
        )?;
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        let complete = self.partial.with_extension("jsonl");
        ensure!(!complete.exists(), "journal destination already exists");
        drop(self.writer);
        std::fs::rename(&self.partial, &complete)?;
        Ok(complete)
    }
}

pub fn start(
    root: &Path,
    metrics: Arc<AppMetrics>,
    shutdown: CancellationToken,
) -> Result<(mpsc::Sender<JournalMessage>, tokio::task::JoinHandle<()>)> {
    let mut journal = Journal::create(root)?;
    tracing::info!(path=%journal.partial.display(), "normalized event recording enabled (bounded, opt-in)");
    let (tx, mut rx) = mpsc::channel::<JournalMessage>(256);
    let task = tokio::task::spawn_blocking(move || {
        let result = (|| -> Result<()> {
            while let Some(message) = rx.blocking_recv() {
                journal.append(
                    message.received_at_ms,
                    metrics.ticks_dropped_total.get(),
                    serde_json::to_value(message.event.as_ref())?,
                )?;
            }
            journal.seal(metrics.ticks_dropped_total.get())?;
            Ok(())
        })();
        if let Err(error) = result {
            tracing::error!(%error,"journal failed; stopping collection rather than claiming complete history");
            shutdown.cancel();
        }
    });
    Ok((tx, task))
}

#[derive(Debug, Serialize)]
pub struct Verification {
    pub records: u64,
    pub source_dropped_total: u64,
    pub sealed: bool,
    pub normalized_not_raw: bool,
}

pub fn verify(path: &Path) -> Result<Verification> {
    visit(path, |_| Ok(()))
}

pub(crate) fn visit(
    path: &Path,
    mut on_record: impl FnMut(&Record) -> Result<()>,
) -> Result<Verification> {
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_BYTES,
        "journal exceeds size limit"
    );
    let mut reader = BufReader::new(file);
    let mut result = Verification {
        records: 0,
        source_dropped_total: 0,
        sealed: false,
        normalized_not_raw: true,
    };
    loop {
        let mut line = String::new();
        let count = std::io::Read::take(&mut reader, MAX_LINE as u64).read_line(&mut line)?;
        if count == 0 {
            break;
        }
        ensure!(
            count < MAX_LINE && line.ends_with('\n'),
            "truncated or oversized journal record"
        );
        let checked: CheckedRecord =
            serde_json::from_str(&line).context("invalid journal envelope")?;
        ensure!(
            checked.crc32 == checksum(checked.body.as_bytes()),
            "journal checksum mismatch"
        );
        let record: Record = serde_json::from_str(&checked.body)?;
        ensure!(
            !result.sealed
                && record.version == "journal/v1"
                && record.sequence == result.records + 1,
            "journal version/sequence/end marker mismatch"
        );
        ensure!(
            record.dropped_total >= result.source_dropped_total,
            "drop counter moved backwards"
        );
        result.records += 1;
        result.source_dropped_total = record.dropped_total;
        result.sealed = record.payload.get("type").and_then(|v| v.as_str()) == Some("session_end");
        on_record(&record)?;
    }
    result.sealed &= path.extension().is_some_and(|ext| ext == "jsonl");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_roundtrip_keeps_sequences_and_drop_evidence() {
        let dir = std::env::temp_dir().join(format!(
            "mb-journal-{}-{}",
            std::process::id(),
            SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut journal = Journal::create(&dir).unwrap();
        journal
            .append(10, 2, serde_json::json!({"type":"fixture"}))
            .unwrap();
        let partial = journal.partial.clone();
        assert!(!verify(&partial).unwrap().sealed);
        let path = journal.seal(3).unwrap();
        let v = verify(&path).unwrap();
        assert!(v.sealed);
        assert_eq!(v.records, 2);
        assert_eq!(v.source_dropped_total, 3);
        // A mutated record cannot pass verification.
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("fixture", "corrupt");
        std::fs::write(&path, text).unwrap();
        assert!(verify(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn entire_journal_replay_uses_record_order_and_rejects_drops() {
        let dir = std::env::temp_dir().join(format!(
            "mb-replay-{}-{}",
            std::process::id(),
            SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut e = crate::research_engine::tests::fixture();
        e.buy.instrument.venue = "binance".into();
        e.sell.instrument.venue = "okx".into();
        let route = crate::api::routes::opportunities::LiveScanRequest {
            buy: e.buy.instrument.clone(),
            sell: e.sell.instrument.clone(),
            relationship: e.relationship.clone(),
            quantities: e.quantities.clone(),
            costs: e.costs.clone(),
            max_age_ms: 1000,
            max_skew_ms: 100,
        };
        for drops in [0, 1] {
            let mut journal = Journal::create(&dir).unwrap();
            for book in [&e.buy, &e.sell] {
                journal.append(10020,drops,serde_json::json!({"type":"order_book","market":"spot","exchange":book.instrument.venue,"symbol":book.instrument.symbol,"ts_ms":10000,"bids":book.bids,"asks":book.asks})).unwrap();
            }
            let path = journal.seal(drops).unwrap();
            let result = crate::journal_replay::replay(&path, route.clone());
            if drops == 0 {
                assert_eq!(result.unwrap()["decision_count"], 1);
            } else {
                assert!(result.is_err());
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
