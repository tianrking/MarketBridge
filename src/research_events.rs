//! Point-in-time announcement ingestion and descriptive event-window studies.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub fn spawn_ingestion(
    bus: crate::event_bus::EventBus,
    control: crate::research_control::ResearchControl,
    store: crate::research_store::ResearchStore,
    stop: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    // Subscribe before spawning, so the receiver exists before collectors run.
    let mut events = bus.subscribe_domain(crate::event_bus::EventDomain::ExternalSignal);
    tokio::spawn(async move {
        loop {
            let event = tokio::select! {_=stop.cancelled()=>break,event=events.recv()=>event};
            let event = match event {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    tracing::warn!(count, "announcement ingestion gap; history is incomplete");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            if !control.announcements_enabled() {
                continue;
            }
            let crate::types::DataEvent::ExternalSignal(signal) = event.event.as_ref() else {
                continue;
            };
            let Some((title, url)) = signal.title.as_deref().zip(signal.url.as_deref()) else {
                continue;
            };
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            let source = signal
                .source_instance
                .as_deref()
                .unwrap_or(signal.source)
                .to_string();
            (&source, title, url).hash(&mut hash);
            let announcement = Announcement {
                id: format!("feed-{:016x}", hash.finish()),
                source,
                source_url: url.into(),
                title: title.into(),
                category: signal.category.to_string(),
                instrument_ids: vec![],
                published_at_ms: None,
                known_at_ms: crate::types::now_ms(),
                effective_at_ms: None,
            };
            let db = store.clone();
            if let Err(error) = announcement.validate(crate::types::now_ms()) {
                tracing::warn!(%error,"invalid external announcement skipped");
                continue;
            }
            let result = tokio::task::spawn_blocking(move || -> Result<()> {
                let _operation = db.operation_lock()?;
                if let Some(existing) = db.get("announcements", &announcement.id)? {
                    let previous: Announcement = serde_json::from_value(existing.payload)?;
                    ensure!(
                        previous.source == announcement.source
                            && previous.title == announcement.title
                            && previous.source_url == announcement.source_url,
                        "announcement ID collision"
                    );
                    return Ok(());
                }
                db.insert(
                    "announcements",
                    &announcement.id,
                    crate::types::now_ms(),
                    &serde_json::to_value(&announcement)?,
                )?;
                Ok(())
            })
            .await;
            if !matches!(result, Ok(Ok(()))) {
                tracing::error!(error=?result,"announcement persistence failed; stopping to avoid silent data loss");
                stop.cancel();
                break;
            }
        }
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Announcement {
    pub id: String,
    pub source: String,
    pub source_url: String,
    pub title: String,
    pub category: String,
    pub instrument_ids: Vec<String>,
    pub published_at_ms: Option<u64>,
    pub known_at_ms: u64,
    pub effective_at_ms: Option<u64>,
}
impl Announcement {
    pub fn validate(&self, as_of: u64) -> Result<()> {
        ensure!(
            crate::research_store::valid_id(&self.id),
            "invalid announcement ID"
        );
        for text in [&self.source, &self.title, &self.category] {
            ensure!(
                !text.trim().is_empty() && text.len() <= 2048,
                "event text empty or too long"
            );
        }
        ensure!(self.source_url.len() <= 2048, "source URL too long");
        let url = reqwest::Url::parse(&self.source_url)?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none(),
            "public source URL required"
        );
        ensure!(
            self.known_at_ms > 0
                && self.known_at_ms <= as_of
                && self
                    .published_at_ms
                    .is_none_or(|t| t > 0 && t <= self.known_at_ms)
                && self.effective_at_ms != Some(0),
            "invalid event knowledge/publication time"
        );
        ensure!(
            self.instrument_ids.len() <= 64
                && self
                    .instrument_ids
                    .iter()
                    .all(|id| !id.trim().is_empty() && id.len() <= 256),
            "invalid affected instruments"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub time_ms: u64,
    pub known_at_ms: u64,
    pub price: f64,
    pub evidence: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventStudy {
    pub announcement: Announcement,
    pub instrument_id: String,
    pub as_of_ms: u64,
    pub horizon_ms: u64,
    pub max_gap_ms: u64,
    pub samples: Vec<Sample>,
}
pub fn study(request: &EventStudy) -> Result<Value> {
    request.announcement.validate(request.as_of_ms)?;
    ensure!(
        !request.instrument_id.trim().is_empty()
            && request.instrument_id.len() <= 256
            && request
                .announcement
                .instrument_ids
                .contains(&request.instrument_id),
        "instrument not included in event"
    );
    ensure!(
        request.horizon_ms > 0
            && request.max_gap_ms > 0
            && (2..=10000).contains(&request.samples.len()),
        "invalid event window/sample count"
    );
    let anchor = request.announcement.known_at_ms;
    let target = anchor
        .checked_add(request.horizon_ms)
        .ok_or_else(|| anyhow::anyhow!("window overflow"))?;
    ensure!(
        target <= request.as_of_ms,
        "event window not yet observable"
    );
    let mut last = 0;
    for sample in &request.samples {
        ensure!(
            sample.time_ms > last
                && sample.time_ms <= sample.known_at_ms
                && sample.known_at_ms <= request.as_of_ms
                && sample.price.is_finite()
                && sample.price > 0.0
                && !sample.evidence.trim().is_empty()
                && sample.evidence.len() <= 2048,
            "invalid/unordered/future sample"
        );
        last = sample.time_ms;
    }
    // No interpolation using future ticks, and baseline must actually have been known.
    let before = request.samples.iter().rev().find(|s| {
        s.time_ms <= anchor && s.known_at_ms <= anchor && anchor - s.time_ms <= request.max_gap_ms
    });
    let after = request
        .samples
        .iter()
        .find(|s| s.time_ms >= target && s.time_ms - target <= request.max_gap_ms);
    let change = before
        .zip(after)
        .map(|(a, b)| (b.price / a.price - 1.0) * 10000.0);
    ensure!(
        change.is_none_or(f64::is_finite),
        "return arithmetic overflow"
    );
    Ok(
        json!({"model_version":"announcement-window/v1","status":if change.is_some(){"descriptive_only"}else{"insufficient_data"},"event_id":request.announcement.id,"anchor_ms":anchor,"target_ms":target,"baseline_ms":before.map(|s|s.time_ms),"end_ms":after.map(|s|s.time_ms),"return_bps":change,"conditional_net_profit":null,"limitations":["descriptive association, not event causality or a trading backtest","event knowledge and sample evidence supplied by caller; publication time is not first observation","no fills, fees, benchmark adjustment or recommendation"]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_baseline_cannot_use_late_known_price() {
        let mut request = EventStudy {
            announcement: Announcement {
                id: "event".into(),
                source: "fixture".into(),
                source_url: "https://example.test/news".into(),
                title: "fixture".into(),
                category: "listing".into(),
                instrument_ids: vec!["btc".into()],
                published_at_ms: Some(10),
                known_at_ms: 20,
                effective_at_ms: Some(30),
            },
            instrument_id: "btc".into(),
            as_of_ms: 100,
            horizon_ms: 10,
            max_gap_ms: 10,
            samples: vec![
                Sample {
                    time_ms: 19,
                    known_at_ms: 21,
                    price: 100.0,
                    evidence: "a".into(),
                },
                Sample {
                    time_ms: 30,
                    known_at_ms: 30,
                    price: 101.0,
                    evidence: "b".into(),
                },
            ],
        };
        assert_eq!(study(&request).unwrap()["status"], "insufficient_data");
        request.samples[0].known_at_ms = 19;
        assert!((study(&request).unwrap()["return_bps"].as_f64().unwrap() - 100.0).abs() < 1e-9);
        request.as_of_ms = 29;
        assert!(study(&request).is_err());
    }
}
