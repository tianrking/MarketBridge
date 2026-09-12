//! Append-only research documents; local SQLite transactions, never order state.
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone)]
pub struct ResearchStore(Arc<Mutex<Connection>>, Arc<Mutex<()>>);

#[derive(Debug, Serialize)]
pub struct Document {
    pub sequence: u64,
    pub namespace: String,
    pub id: String,
    pub recorded_at_ms: u64,
    pub payload: Value,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 120
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

impl ResearchStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }
    fn from_connection(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(3))?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS research_documents(
              sequence INTEGER PRIMARY KEY AUTOINCREMENT,
              namespace TEXT NOT NULL,id TEXT NOT NULL,recorded_at_ms INTEGER NOT NULL,
              body TEXT NOT NULL,crc INTEGER NOT NULL,UNIQUE(namespace,id));",
        )?;
        let page_size: u64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        conn.pragma_update(None, "max_page_count", (512 * 1024 * 1024u64) / page_size)?;
        Ok(Self(Arc::new(Mutex::new(conn)), Arc::new(Mutex::new(()))))
    }
    pub fn operation_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.1
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace operation lock poisoned"))
    }
    #[cfg(test)]
    pub fn memory() -> Self {
        Self::from_connection(Connection::open_in_memory().unwrap()).unwrap()
    }

    pub fn insert(&self, namespace: &str, id: &str, at: u64, payload: &Value) -> Result<Document> {
        ensure!(
            valid_id(namespace) && valid_id(id),
            "invalid document namespace or ID"
        );
        ensure!(at > 0 && at <= i64::MAX as u64, "invalid record time");
        let body = serde_json::to_string(payload)?;
        ensure!(body.len() <= 2 * 1024 * 1024, "document exceeds 2 MiB");
        let conn = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("research store lock poisoned"))?;
        conn.execute("INSERT INTO research_documents(namespace,id,recorded_at_ms,body,crc) VALUES(?1,?2,?3,?4,?5)",params![namespace,id,at,body,checksum(body.as_bytes())])?;
        Ok(Document {
            sequence: conn.last_insert_rowid() as u64,
            namespace: namespace.into(),
            id: id.into(),
            recorded_at_ms: at,
            payload: payload.clone(),
        })
    }
    pub fn get(&self, namespace: &str, id: &str) -> Result<Option<Document>> {
        let conn = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("research store lock poisoned"))?;
        let row=conn.query_row("SELECT sequence,namespace,id,recorded_at_ms,body,crc FROM research_documents WHERE namespace=?1 AND id=?2",params![namespace,id],read_row).optional()?;
        row.map(decode).transpose()
    }
    pub fn list(&self, namespace: &str, after: u64, limit: usize) -> Result<Vec<Document>> {
        ensure!(after <= i64::MAX as u64, "cursor out of range");
        let conn = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("research store lock poisoned"))?;
        let mut stmt=conn.prepare("SELECT sequence,namespace,id,recorded_at_ms,body,crc FROM research_documents WHERE namespace=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
        let mut output = Vec::new();
        let mut bytes = 0;
        for row in stmt.query_map(params![namespace, after, limit.clamp(1, 100)], read_row)? {
            let row = row?;
            bytes += row.4.len();
            if bytes > 4 * 1024 * 1024 && !output.is_empty() {
                break;
            }
            output.push(decode(row)?);
        }
        Ok(output)
    }
    pub fn tail_and_count(&self, namespace: &str) -> Result<(Option<Document>, usize)> {
        let conn = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("research store lock poisoned"))?;
        let count = conn.query_row(
            "SELECT count(*) FROM research_documents WHERE namespace=?1",
            [namespace],
            |r| r.get(0),
        )?;
        let row=conn.query_row("SELECT sequence,namespace,id,recorded_at_ms,body,crc FROM research_documents WHERE namespace=?1 ORDER BY sequence DESC LIMIT 1",[namespace],read_row).optional()?;
        Ok((row.map(decode).transpose()?, count))
    }
    pub fn integrity(&self) -> Result<Value> {
        let conn = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("research store lock poisoned"))?;
        let sqlite: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        let count: u64 =
            conn.query_row("SELECT count(*) FROM research_documents", [], |r| r.get(0))?;
        Ok(
            serde_json::json!({"sqlite":sqlite,"documents":count,"payload_crc_checked_on_read":true,"authenticity_attested":false}),
        )
    }
}
type Row = (u64, String, String, u64, String, u32);
fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}
fn decode((sequence, namespace, id, recorded_at_ms, body, crc): Row) -> Result<Document> {
    ensure!(
        checksum(body.as_bytes()) == crc,
        "research document checksum mismatch"
    );
    Ok(Document {
        sequence,
        namespace,
        id,
        recorded_at_ms,
        payload: serde_json::from_str(&body)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_documents_and_cursor_roundtrip() {
        let db = ResearchStore::memory();
        let value = serde_json::json!({"hello":"world"});
        let a = db.insert("runs", "a", 1, &value).unwrap();
        assert!(db.insert("runs", "a", 2, &value).is_err());
        let b = db.insert("runs", "b", 2, &value).unwrap();
        assert_eq!(
            db.list("runs", a.sequence, 10).unwrap()[0].sequence,
            b.sequence
        );
        assert_eq!(db.get("runs", "a").unwrap().unwrap().payload, value);
        assert_eq!(db.integrity().unwrap()["sqlite"], "ok");
    }
    #[test]
    fn corruption_is_not_silently_returned() {
        let db = ResearchStore::memory();
        db.insert("runs", "a", 1, &serde_json::json!({})).unwrap();
        db.0.lock()
            .unwrap()
            .execute("UPDATE research_documents SET body='{}x'", [])
            .unwrap();
        assert!(db.get("runs", "a").is_err());
    }
}
