//! Read-only market scanning, durable state-change alerts and transactional reload.
use crate::{
    api::routes::opportunities::{LiveScanRequest, cached_evidence},
    event_bus::EventBus,
    research_store::ResearchStore,
    types::now_ms,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanRoute {
    pub id: String,
    pub route: LiveScanRequest,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlConfig {
    pub revision: String,
    pub enabled: bool,
    pub interval_ms: u64,
    pub min_net_bps: f64,
    pub min_hold_ms: u64,
    pub record_scans: bool,
    #[serde(default)]
    pub record_announcements: bool,
    pub routes: Vec<ScanRoute>,
}
impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            revision: "disabled".into(),
            enabled: false,
            interval_ms: 5000,
            min_net_bps: 0.0,
            min_hold_ms: 5000,
            record_scans: false,
            record_announcements: false,
            routes: vec![],
        }
    }
}
impl ControlConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            crate::research_store::valid_id(&self.revision),
            "invalid control revision"
        );
        ensure!(
            (1000..=60000).contains(&self.interval_ms)
                && self.min_hold_ms <= 3600000
                && self.min_net_bps.is_finite()
                && self.min_net_bps >= 0.0,
            "invalid scanner interval/threshold/hold"
        );
        ensure!(
            self.routes.len() <= 64 && (!self.enabled || !self.routes.is_empty()),
            "enabled scanner requires 1..64 routes"
        );
        let mut ids = std::collections::HashSet::new();
        for item in &self.routes {
            ensure!(
                crate::research_store::valid_id(&item.id) && ids.insert(&item.id),
                "invalid or duplicate route ID"
            );
            let r = &item.route;
            r.buy.validate().map_err(anyhow::Error::msg)?;
            r.sell.validate().map_err(anyhow::Error::msg)?;
            ensure!(
                r.max_age_ms > 0
                    && !r.quantities.is_empty()
                    && r.quantities.len() <= 32
                    && r.quantities.iter().all(|v| v.is_finite() && *v > 0.0),
                "invalid route quantities/TTL"
            );
            ensure!(
                !r.costs.version.trim().is_empty() && r.costs.version.len() <= 256,
                "cost version required"
            );
            ensure!(
                [r.costs.buy_fee_bps, r.costs.sell_fee_bps]
                    .into_iter()
                    .flatten()
                    .all(|v| v.is_finite() && (0.0..=10000.0).contains(&v))
                    && r.costs
                        .other_cost_quote
                        .is_none_or(|v| v.is_finite() && v >= 0.0),
                "invalid route costs"
            );
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ResearchControl {
    config: Arc<RwLock<ControlConfig>>,
    status: Arc<RwLock<Value>>,
    reload_error: Arc<RwLock<Value>>,
    sequence: Arc<AtomicU64>,
    store: ResearchStore,
}
impl ResearchControl {
    pub fn announcements_enabled(&self) -> bool {
        self.config.read().unwrap().record_announcements
    }
    pub fn new(store: ResearchStore) -> Result<Self> {
        let (last, _) = store.tail_and_count("control")?;
        let config = last
            .map(|d| serde_json::from_value(d.payload))
            .transpose()?
            .unwrap_or_default();
        let this = Self {
            config: Arc::new(RwLock::new(config)),
            status: Arc::new(RwLock::new(json!({"state":"starting"}))),
            reload_error: Arc::new(RwLock::new(Value::Null)),
            sequence: Arc::new(AtomicU64::new(0)),
            store,
        };
        this.config.read().unwrap().validate()?;
        Ok(this)
    }
    pub fn apply(&self, config: ControlConfig) -> Result<Value> {
        config.validate()?;
        let _operation = self.store.operation_lock()?;
        let payload = serde_json::to_value(&config)?;
        if let Some(existing) = self.store.get("control", &config.revision)? {
            ensure!(
                existing.payload == payload,
                "revision already exists with different content"
            );
            ensure!(
                self.store
                    .tail_and_count("control")?
                    .0
                    .is_some_and(|d| d.sequence == existing.sequence),
                "rollback requires a new revision ID"
            );
        } else {
            self.store
                .insert("control", &config.revision, now_ms(), &payload)?;
        }
        *self
            .config
            .write()
            .map_err(|_| anyhow::anyhow!("control lock poisoned"))? = config;
        Ok(json!({"applied":payload,"orders_supported":false}))
    }
    pub fn status(&self) -> Value {
        json!({"config":*self.config.read().unwrap(),"latest":*self.status.read().unwrap(),"reload_error":*self.reload_error.read().unwrap(),"orders_supported":false})
    }
    fn record(&self, namespace: &str, payload: Value) -> Result<()> {
        let id = format!(
            "{}-{}-{}",
            now_ms(),
            std::process::id(),
            self.sequence.fetch_add(1, Ordering::Relaxed)
        );
        self.store.insert(namespace, &id, now_ms(), &payload)?;
        Ok(())
    }
    pub fn spawn(&self, bus: EventBus, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let path = std::env::var_os("MARKETBRIDGE_CONTROL_FILE").map(std::path::PathBuf::from);
            let mut last_file = String::new();
            let mut revision = String::new();
            let mut since = std::collections::HashMap::<String, Instant>::new();
            let mut active = std::collections::BTreeSet::new();
            let mut next = Instant::now();
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {_=stop.cancelled()=>break,_=tick.tick()=>{}}
                if let Some(path) = path.clone() {
                    let read = tokio::task::spawn_blocking(move || -> Result<String> {
                        let file = std::fs::File::open(path)?;
                        ensure!(
                            file.metadata()?.len() <= 2 * 1024 * 1024,
                            "control file exceeds 2 MiB"
                        );
                        let mut text = String::new();
                        std::io::Read::read_to_string(
                            &mut std::io::Read::take(file, 2 * 1024 * 1024 + 1),
                            &mut text,
                        )?;
                        ensure!(
                            text.len() <= 2 * 1024 * 1024,
                            "control file grew beyond limit"
                        );
                        Ok(text)
                    })
                    .await;
                    match read {
                        Ok(Ok(text)) if text != last_file => {
                            last_file = text.clone();
                            let loader = this.clone();
                            let applied = tokio::task::spawn_blocking(move || {
                                serde_json::from_str::<ControlConfig>(&text)
                                    .map_err(anyhow::Error::from)
                                    .and_then(|c| loader.apply(c))
                            })
                            .await;
                            if let Err(error) = applied.unwrap_or_else(|e| Err(e.into())) {
                                *this.reload_error.write().unwrap() = json!({"state":"reload_rejected","error":error.to_string(),"previous_config_retained":true});
                            } else {
                                *this.reload_error.write().unwrap() = Value::Null;
                            }
                        }
                        Ok(Err(error)) => {
                            *this.reload_error.write().unwrap() = json!({"state":"reload_read_error","error":error.to_string(),"previous_config_retained":true});
                        }
                        _ => {}
                    }
                }
                let config = this.config.read().unwrap().clone();
                if revision != config.revision {
                    revision = config.revision.clone();
                    since.clear();
                    active.clear();
                    next = Instant::now();
                    *this.status.write().unwrap() = json!({"state":if config.enabled {"ready"} else {"disabled"},"revision":revision});
                }
                if !config.enabled || Instant::now() < next {
                    continue;
                }
                next = Instant::now() + Duration::from_millis(config.interval_ms);
                let engine = this.clone();
                let cache = bus.clone();
                let scanned =
                    tokio::task::spawn_blocking(move || scan(&cache, &config).map(|v| (config, v)))
                        .await;
                let (config, (output, qualified)) = match scanned {
                    Ok(Ok(result)) => result,
                    other => {
                        *this.status.write().unwrap() =
                            json!({"state":"scan_failed","error":format!("{other:?}")});
                        continue;
                    }
                };
                if this.config.read().unwrap().revision != config.revision {
                    continue;
                }
                since.retain(|id, _| qualified.contains(id));
                let mut held = std::collections::BTreeSet::new();
                for id in qualified {
                    if since
                        .entry(id.clone())
                        .or_insert_with(Instant::now)
                        .elapsed()
                        >= Duration::from_millis(config.min_hold_ms)
                    {
                        held.insert(id);
                    }
                }
                let changed = held != active;
                active = held;
                *this.status.write().unwrap() = json!({"state":"running","as_of_ms":now_ms(),"revision":revision,"qualified_after_hold":active,"scan":output});
                let status = this.status.read().unwrap().clone();
                let persisted=tokio::task::spawn_blocking(move||->Result<()>{
                    if changed {engine.record("events",json!({"kind":"scanner_state_change","status":status,"not_an_execution_signal":true}))?;}
                    if config.record_scans {engine.record("captures",status)?;}
                    Ok(())
                }).await;
                if let Err(error) = persisted.unwrap_or_else(|e| Err(e.into())) {
                    this.config.write().unwrap().enabled = false;
                    *this.status.write().unwrap() =
                        json!({"state":"paused_storage_error","error":error.to_string()});
                }
            }
        })
    }
}
fn scan(
    bus: &EventBus,
    config: &ControlConfig,
) -> Result<(Value, std::collections::BTreeSet<String>)> {
    let mut rows = Vec::new();
    let mut qualified = std::collections::BTreeSet::new();
    for item in &config.routes {
        let route = &item.route;
        let evidence = cached_evidence(bus, route.buy.clone()).and_then(|buy| {
            cached_evidence(bus, route.sell.clone()).map(|sell| {
                crate::research_engine::ScanRequest {
                    as_of_ms: now_ms(),
                    max_age_ms: route.max_age_ms,
                    max_skew_ms: route.max_skew_ms,
                    relationship: route.relationship.clone(),
                    buy,
                    sell,
                    quantities: route.quantities.clone(),
                    costs: route.costs.clone(),
                }
            })
        });
        match evidence {
            Ok(input) => match crate::research_engine::scan(&input) {
                Ok(result) => {
                    if result.points.iter().any(|p| {
                        p.conditional_net_quote.is_some_and(|v| v > 0.0)
                            && p.net_bps_on_buy_notional
                                .is_some_and(|v| v >= config.min_net_bps)
                    }) {
                        qualified.insert(item.id.clone());
                    }
                    rows.push(json!({"id":item.id,"input":input,"result":result}));
                }
                Err(error) => rows.push(json!({"id":item.id,"error":error})),
            },
            Err(error) => rows.push(json!({"id":item.id,"error":error})),
        }
    }
    Ok((
        json!({"routes":rows,"each_route_has_own_decision_time":true}),
        qualified,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_reload_retains_previous_config_and_duplicate_revision_fails() {
        let control = ResearchControl::new(ResearchStore::memory()).unwrap();
        let valid = ControlConfig {
            revision: "one".into(),
            ..Default::default()
        };
        control.apply(valid.clone()).unwrap();
        control.apply(valid).unwrap();
        let invalid = ControlConfig {
            revision: "two".into(),
            interval_ms: 0,
            ..Default::default()
        };
        assert!(control.apply(invalid).is_err());
        assert_eq!(control.status()["config"]["revision"], "one");
        control
            .apply(ControlConfig {
                revision: "two".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(
            control
                .apply(ControlConfig {
                    revision: "one".into(),
                    ..Default::default()
                })
                .is_err()
        );
    }

    #[tokio::test]
    async fn scanner_records_missing_data_without_false_alerts() {
        let db = ResearchStore::memory();
        let control = ResearchControl::new(db.clone()).unwrap();
        let e = crate::research_engine::tests::fixture();
        control
            .apply(ControlConfig {
                revision: "running".into(),
                enabled: true,
                interval_ms: 1000,
                min_hold_ms: 0,
                record_scans: true,
                routes: vec![ScanRoute {
                    id: "fixture".into(),
                    route: LiveScanRequest {
                        buy: e.buy.instrument,
                        sell: e.sell.instrument,
                        relationship: e.relationship,
                        quantities: e.quantities,
                        costs: e.costs,
                        max_age_ms: 1000,
                        max_skew_ms: 100,
                    },
                }],
                ..Default::default()
            })
            .unwrap();
        let stop = CancellationToken::new();
        let task = control.spawn(EventBus::new_sharded(16, 1000, 1), stop.clone());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if db.tail_and_count("captures").unwrap().1 > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        stop.cancel();
        task.await.unwrap();
        assert_eq!(db.tail_and_count("events").unwrap().1, 0);
        assert!(control.status()["latest"]["scan"]["routes"][0]["error"].is_string());
    }
}
