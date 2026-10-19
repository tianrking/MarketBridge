//! Observed source readiness and protocol-specific continuity. Receipt age uses
//! Instant; source time is reported separately and never substitutes for it.
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::types::{DataEvent, MarketKind, TradeSide, now_ms};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct SourceIntegrity {
    pub observation_origin: &'static str,
    pub scope: &'static str,
    pub has_known_loss: bool,
    pub loss_epoch: u64,
    pub reasons: BTreeMap<&'static str, u64>,
    pub upstream_completeness: &'static str,
}

impl Default for SourceIntegrity {
    fn default() -> Self {
        Self {
            observation_origin: crate::outbox::EventIdentity::process_origin(),
            scope: "process_ingestion_observation",
            has_known_loss: false,
            loss_epoch: 0,
            reasons: BTreeMap::new(),
            upstream_completeness: "not_verified",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeCheckpoint {
    pub key: String,
    pub sequence: u64,
    pub signature: [u64; 4],
}

impl NativeCheckpoint {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.key.len() <= 512
                && (self.key.starts_with("binance:trade:")
                    || self.key.starts_with("acceptance_fixture:trade:"))
                && f64::from_bits(self.signature[0]).is_finite()
                && f64::from_bits(self.signature[0]) > 0.0
                && f64::from_bits(self.signature[1]).is_finite()
                && f64::from_bits(self.signature[1]) > 0.0
                && self.signature[2] > 0
                && (1..=2).contains(&self.signature[3]),
            "native checkpoint contract invalid"
        );
        Ok(())
    }
}

pub fn native_checkpoint(event: &DataEvent) -> Option<NativeCheckpoint> {
    if let DataEvent::Trade(t) = event
        && matches!(t.exchange, "binance" | "acceptance_fixture")
    {
        let sequence = t.trade_id.as_deref()?.parse::<u64>().ok()?;
        if !t.price.is_finite()
            || !t.qty.is_finite()
            || t.price <= 0.0
            || t.qty <= 0.0
            || t.ts_ms == 0
            || t.side == TradeSide::Unknown
        {
            return None;
        }
        let side = match t.side {
            TradeSide::Buy => 1,
            TradeSide::Sell => 2,
            TradeSide::Unknown => 0,
        };
        return Some(NativeCheckpoint {
            key: stream_key(event),
            sequence,
            signature: [t.price.to_bits(), t.qty.to_bits(), t.ts_ms, side],
        });
    }
    None
}

pub fn domain(event: &DataEvent) -> &'static str {
    match event {
        DataEvent::Tick(_) => "quote",
        DataEvent::Trade(_) => "trade",
        DataEvent::OrderBook(_) => "order_book",
        DataEvent::FundingRate(_) => "funding",
        DataEvent::OpenInterest(_) => "open_interest",
        DataEvent::Liquidation(_) => "liquidation",
        DataEvent::ExternalSignal(_) => "external_signal",
        DataEvent::Heartbeat { .. } => "heartbeat",
    }
}

pub fn source_time(event: &DataEvent) -> u64 {
    match event {
        DataEvent::Tick(t) => t.ts_ms,
        DataEvent::Trade(t) => t.ts_ms,
        DataEvent::OrderBook(t) => t.ts_ms,
        DataEvent::FundingRate(t) => t.ts_ms,
        DataEvent::OpenInterest(t) => t.ts_ms,
        DataEvent::Liquidation(t) => t.ts_ms,
        DataEvent::ExternalSignal(t) => t.ts_ms,
        DataEvent::Heartbeat { ts_ms, .. } => *ts_ms,
    }
}

pub fn stream_key(event: &DataEvent) -> String {
    let market = |m| match m {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "perp",
    };
    match event {
        DataEvent::Tick(t) => format!("{}:quote:{}:{}", t.exchange, market(t.market), t.symbol),
        DataEvent::Trade(t) => format!("{}:trade:{}:{}", t.exchange, market(t.market), t.symbol),
        DataEvent::OrderBook(t) => format!(
            "{}:order_book:{}:{}",
            t.exchange,
            market(t.market),
            t.symbol
        ),
        DataEvent::FundingRate(t) => format!("{}:funding:{}", t.exchange, t.symbol),
        DataEvent::OpenInterest(t) => format!("{}:open_interest:{}", t.exchange, t.symbol),
        DataEvent::Liquidation(t) => format!("{}:liquidation:{}", t.exchange, t.symbol),
        DataEvent::ExternalSignal(t) => format!(
            "{}:external_signal:{}:{}",
            t.source,
            t.category,
            t.symbol.as_deref().unwrap_or("*")
        ),
        DataEvent::Heartbeat { exchange, .. } => format!("{exchange}:heartbeat"),
    }
}

#[derive(Debug, Clone)]
pub struct EventTrace {
    pub received_at: Instant,
    pub source_id: Arc<str>,
    pub attempt: u64,
    pub source_quality: SourceIntegrity,
}

struct ObservedSource {
    label: String,
    phase: &'static str,
    attempt: u64,
    received: Option<Instant>,
    heartbeat: Option<Instant>,
    received_at_ms: Option<u64>,
    source_time_ms: Option<u64>,
    published: Option<Instant>,
    stages: BTreeMap<&'static str, Instant>,
    admitted: BTreeMap<&'static str, Instant>,
    saw_trade: bool,
    events: u64,
    accepted: u64,
    duplicates: u64,
    last_error: Option<String>,
}

struct StreamObservation {
    receipt: Instant,
    source_id: Arc<str>,
    source_time_ms: u64,
}

struct State {
    sources: BTreeMap<String, ObservedSource>,
    streams: BTreeMap<String, StreamObservation>,
    native: HashMap<String, NativeCheckpoint>,
    integrity: SourceIntegrity,
    config: crate::config::runtime::ReadinessConfig,
    max_streams: usize,
    stopping: bool,
}

#[derive(Clone)]
pub struct IngestionState {
    inner: Arc<Mutex<State>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    Accept(u64),
    Duplicate,
    Quarantine,
}

impl Default for IngestionState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                sources: BTreeMap::new(),
                streams: BTreeMap::new(),
                native: HashMap::new(),
                integrity: SourceIntegrity {
                    upstream_completeness: "not_verified",
                    ..Default::default()
                },
                config: Default::default(),
                max_streams: 4096,
                stopping: false,
            })),
        }
    }
}

impl IngestionState {
    pub fn configure(&self, config: crate::config::runtime::ReadinessConfig, max_streams: usize) {
        let mut s = self.inner.lock().unwrap();
        s.config = config;
        s.max_streams = max_streams;
    }
    pub fn register(&self, id: &str, label: &str) -> anyhow::Result<()> {
        let mut s = self.inner.lock().unwrap();
        anyhow::ensure!(
            s.sources.len() < 1024 && !s.sources.contains_key(id),
            "source readiness registry exceeds budget or duplicate source id"
        );
        s.sources.insert(
            id.into(),
            ObservedSource {
                label: label.into(),
                phase: "awaiting_first_event",
                attempt: 0,
                received: None,
                heartbeat: None,
                received_at_ms: None,
                source_time_ms: None,
                published: None,
                stages: BTreeMap::new(),
                admitted: BTreeMap::new(),
                saw_trade: false,
                events: 0,
                accepted: 0,
                duplicates: 0,
                last_error: None,
            },
        );
        Ok(())
    }
    pub fn install_checkpoints(&self, rows: Vec<NativeCheckpoint>) -> anyhow::Result<()> {
        let mut s = self.inner.lock().unwrap();
        anyhow::ensure!(
            rows.len() <= s.max_streams,
            "persisted native checkpoints exceed tracker budget"
        );
        for row in rows {
            row.validate()?;
            s.native.insert(row.key.clone(), row);
        }
        Ok(())
    }
    pub fn starting(&self, id: &str) {
        let mut s = self.inner.lock().unwrap();
        if let Some(row) = s.sources.get_mut(id) {
            row.attempt += 1;
            row.phase = "awaiting_first_event";
            row.published = None;
            row.stages.clear();
            row.admitted.clear();
        }
    }
    pub fn retrying(&self, id: &str, error: &str) {
        let mut s = self.inner.lock().unwrap();
        if let Some(row) = s.sources.get_mut(id) {
            row.phase = "retrying";
            row.last_error = Some(error.chars().take(512).collect());
        }
    }
    pub fn stopping(&self) {
        self.inner.lock().unwrap().stopping = true;
    }
    pub fn integrity(&self) -> SourceIntegrity {
        self.inner.lock().unwrap().integrity.clone()
    }
    pub fn loss(&self, reason: &'static str, minimum: u64) {
        let mut s = self.inner.lock().unwrap();
        record_loss(&mut s, reason, minimum);
    }
    pub fn observe(&self, id: &str, event: &DataEvent, receipt: Instant, wall: u64) -> Admission {
        let mut s = self.inner.lock().unwrap();
        let Some(source) = s.sources.get_mut(id) else {
            record_loss(&mut s, "unregistered_source", 1);
            return Admission::Quarantine;
        };
        let attempt = source.attempt;
        source.events += 1;
        if matches!(event, DataEvent::Heartbeat { .. }) {
            source.heartbeat = Some(receipt);
            return Admission::Accept(attempt);
        }
        if source_time(event) > wall.saturating_add(5000)
            || (source_time(event) == 0
                && matches!(event,DataEvent::Trade(t) if matches!(t.exchange,"binance"|"acceptance_fixture")))
        {
            record_loss(&mut s, "invalid_source_clock", 1);
            return Admission::Quarantine;
        }
        source.received = Some(receipt);
        source.received_at_ms = Some(wall);
        source.source_time_ms = Some(source_time(event));
        source.saw_trade |= matches!(event, DataEvent::Trade(_));
        source.phase = "observed_data";
        source.last_error = None;
        let key = stream_key(event);
        if !s.streams.contains_key(&key) && s.streams.len() >= s.max_streams {
            let oldest = s
                .streams
                .iter()
                .min_by_key(|(key, row)| (row.receipt, *key))
                .map(|(key, _)| key.clone())
                .unwrap();
            s.streams.remove(&oldest);
            record_loss(&mut s, "stream_tracker_capacity", 1);
        }
        if s.streams
            .get(&key)
            .is_some_and(|old| source_time(event) > 0 && old.source_time_ms > source_time(event))
        {
            record_loss(&mut s, "source_timestamp_regression", 1);
            return Admission::Quarantine;
        }
        s.streams.insert(
            key.clone(),
            StreamObservation {
                receipt,
                source_id: Arc::from(id),
                source_time_ms: source_time(event),
            },
        );
        let native_expected = matches!(event,DataEvent::Trade(t) if matches!(t.exchange,"binance"|"acceptance_fixture"));
        if let Some(point) = native_checkpoint(event) {
            if let Some(previous) = s.native.get(&key) {
                if point.sequence == previous.sequence && point.signature == previous.signature {
                    s.sources.get_mut(id).unwrap().duplicates += 1;
                    return Admission::Duplicate;
                }
                if point.sequence <= previous.sequence {
                    record_loss(&mut s, "native_sequence_regression_or_collision", 1);
                    return Admission::Quarantine;
                }
                let missing = point.sequence - previous.sequence - 1;
                if missing > 0 {
                    record_loss(&mut s, "native_sequence_gap", missing);
                }
            } else if s.native.len() >= s.max_streams {
                record_loss(&mut s, "native_tracker_capacity", 1);
                return Admission::Quarantine;
            }
            s.native.insert(key, point);
        } else if native_expected {
            record_loss(&mut s, "native_trade_contract_violation", 1);
            return Admission::Quarantine;
        }
        if matches!(event,DataEvent::Trade(t) if t.side==TradeSide::Unknown) {
            record_loss(&mut s, "trade_direction_unavailable", 1);
        }
        Admission::Accept(attempt)
    }
    pub fn native_sequence(&self, key: &str) -> Option<u64> {
        self.inner
            .lock()
            .unwrap()
            .native
            .get(key)
            .map(|p| p.sequence)
    }

    pub fn accepted(&self, id: &str, receipt: Instant, attempt: u64, domain: &str) {
        if let Some(row) = self.inner.lock().unwrap().sources.get_mut(id)
            && row.attempt == attempt
        {
            row.accepted += 1;
            for stage in ["bus", "strategy_state", "aggregator"] {
                row.admitted
                    .entry(stage)
                    .and_modify(|at| *at = (*at).max(receipt))
                    .or_insert(receipt);
            }
            if domain == "trade" {
                row.admitted
                    .entry("order_flow")
                    .and_modify(|at| *at = (*at).max(receipt))
                    .or_insert(receipt);
            }
        }
    }
    pub fn processed(&self, trace: &EventTrace, stage: &'static str) {
        let mut s = self.inner.lock().unwrap();
        if let Some(row) = s.sources.get_mut(trace.source_id.as_ref())
            && row.attempt == trace.attempt
        {
            if stage == "bus" {
                row.published = Some(trace.received_at);
            }
            row.stages
                .entry(stage)
                .and_modify(|at| *at = (*at).max(trace.received_at))
                .or_insert(trace.received_at);
        }
    }
    pub fn report(&self) -> serde_json::Value {
        let s = self.inner.lock().unwrap();
        let now = Instant::now();
        let age = |at: Instant| {
            now.saturating_duration_since(at)
                .as_millis()
                .min(u64::MAX as u128) as u64
        };
        let fresh = |at: Option<Instant>, bound: u64| at.is_some_and(|at| age(at) <= bound);
        let wall = now_ms();
        let rows=s.sources.iter().map(|(id,row)| {
            let bound=*s.config.source_max_age_ms.get(id).or_else(||s.config.source_max_age_ms.get(&row.label)).unwrap_or(&s.config.max_age_ms);
            let caught_up=|stage:&str| row.admitted.get(stage).is_none_or(|expected|
                row.stages.get(stage).is_some_and(|done|done>=expected || fresh(Some(*done),bound)));
            let analytics=fresh(row.stages.get("strategy_state").copied(),bound) && caught_up("strategy_state") && caught_up("order_flow") && caught_up("aggregator");
            let source_time_ready=row.source_time_ms.is_some_and(|ts|ts>0 && ts<=wall.saturating_add(5000) && wall.saturating_sub(ts)<=bound);
            let ready=row.phase=="observed_data" && fresh(row.received,bound) && fresh(row.published,bound) && caught_up("bus") && analytics && source_time_ready;
            serde_json::json!({"id":id,"label":row.label,"phase":if row.phase=="observed_data" && !fresh(row.received,bound) {"quiet_or_stalled"} else {row.phase},"attempt":row.attempt,"observed_events":row.events,"accepted_events":row.accepted,
                "transport_heartbeat_age_ms":row.heartbeat.map(age),"fresh_transport_evidence":fresh(row.heartbeat,bound),
                "duplicates_suppressed":row.duplicates,"last_received_at_ms":row.received_at_ms,"receipt_age_ms":row.received.map(age),
                "last_source_time_ms":row.source_time_ms,"source_time_ready":source_time_ready,"max_age_ms":bound,"source_time_semantics":"adapter_reported_not_independently_verified","published_receipt_age_ms":row.published.map(age),"analytics_ready":analytics,"ready":ready,"last_error":row.last_error})
        }).collect::<Vec<_>>();
        let required=s.config.required_streams.iter().map(|(key,bound)| {
            let observed=s.streams.get(key);
            serde_json::json!({"stream":key,"ready":observed.is_some_and(|o|age(o.receipt)<=*bound && o.source_time_ms>0 && o.source_time_ms<=wall.saturating_add(5000) && wall.saturating_sub(o.source_time_ms)<=*bound),
                "receipt_age_ms":observed.map(|o|age(o.receipt)),"source_id":observed.map(|o|o.source_id.as_ref()),"source_time_ms":observed.map(|o|o.source_time_ms)})
        }).collect::<Vec<_>>();
        let all_sources = rows.iter().all(|row| row["ready"] == true);
        let required_ready = required.iter().all(|row| row["ready"] == true);
        serde_json::json!({"scope":"configured source observations and explicit required streams","generated_at_ms":now_ms(),"stopping":s.stopping,
            "operational_ready":!s.stopping && all_sources && required_ready,
            "require_complete_analytics":s.config.require_complete_analytics,
            "ready":!s.stopping && all_sources && required_ready && !s.integrity.has_known_loss,
            "research_only":s.sources.is_empty(),"data_ready":!s.sources.is_empty() && !s.stopping && all_sources && required_ready && !s.integrity.has_known_loss,
            "max_age_ms":s.config.max_age_ms,"tracked_streams":s.streams.len(),"native_checkpoints":s.native.len(),"integrity":s.integrity,
            "sources":rows,"required_streams":required,"coverage_note":"Source-level freshness does not certify every market; declare required_streams for critical symbols. Native continuity is checked only for supported trade protocols; depth snapshot update IDs are not assumed contiguous."})
    }
}

fn record_loss(s: &mut State, reason: &'static str, minimum: u64) {
    s.integrity.has_known_loss = true;
    s.integrity.loss_epoch = s.integrity.loss_epoch.saturating_add(1);
    let count = s.integrity.reasons.entry(reason).or_default();
    *count = count.saturating_add(minimum.max(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BookLevel, OrderBookTick, TradeTick};
    fn setup() -> IngestionState {
        let s = IngestionState::default();
        s.register("source#0", "source").unwrap();
        s.starting("source#0");
        s
    }
    fn trade(id: u64, ts: u64) -> DataEvent {
        DataEvent::Trade(TradeTick {
            exchange: "binance",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 1.0,
            side: TradeSide::Buy,
            trade_id: Some(id.to_string().into()),
            ts_ms: ts,
        })
    }
    fn book(update: u64, ts: u64) -> DataEvent {
        DataEvent::OrderBook(OrderBookTick {
            exchange: "binance",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            bids: vec![BookLevel {
                price: 99.0,
                qty: 1.0,
            }],
            asks: vec![BookLevel {
                price: 101.0,
                qty: 1.0,
            }],
            last_update_id: Some(update),
            ts_ms: ts,
        })
    }
    fn processed(s: &IngestionState, receipt: Instant, attempt: u64, trade: bool) {
        let trace = EventTrace {
            received_at: receipt,
            source_id: "source#0".into(),
            attempt,
            source_quality: s.integrity(),
        };
        s.accepted(
            "source#0",
            receipt,
            attempt,
            if trade { "trade" } else { "order_book" },
        );
        s.processed(&trace, "bus");
        s.processed(&trace, "aggregator");
        s.processed(&trace, "strategy_state");
        if trade {
            s.processed(&trace, "order_flow");
        }
    }
    #[test]
    fn native_duplicates_gaps_and_regressions_are_distinct() {
        let s = setup();
        let now = now_ms();
        let at = Instant::now();
        assert_eq!(
            s.observe("source#0", &trade(0, now), at, now),
            Admission::Accept(1)
        );
        assert_eq!(
            s.observe("source#0", &trade(0, now), at, now),
            Admission::Duplicate
        );
        assert!(!s.integrity().has_known_loss);
        assert_eq!(
            s.observe("source#0", &trade(3, now), at, now),
            Admission::Accept(1)
        );
        assert_eq!(s.integrity().reasons["native_sequence_gap"], 2);
        assert_eq!(
            s.observe("source#0", &trade(2, now), at, now),
            Admission::Quarantine
        );
        assert!(s.integrity().has_known_loss);
        assert_eq!(
            s.inner.lock().unwrap().native["binance:trade:perp:BTCUSDT"].sequence,
            3
        );
    }
    #[test]
    fn periodic_depth_snapshot_ids_are_not_treated_as_contiguous_sequences() {
        let s = setup();
        let wall = now_ms();
        let at = Instant::now();
        for id in [1, 55, 10000] {
            assert_eq!(
                s.observe("source#0", &book(id, wall), at, wall),
                Admission::Accept(1)
            );
        }
        processed(&s, at, 1, false);
        assert!(!s.integrity().has_known_loss);
        assert_eq!(s.report()["ready"], true);
        assert_eq!(s.report()["native_checkpoints"], 0);
    }
    #[test]
    fn readiness_requires_receipt_publication_consumption_and_current_attempt() {
        let s = setup();
        let wall = now_ms();
        let old = Instant::now() - std::time::Duration::from_secs(61);
        assert_eq!(s.report()["ready"], false);
        s.observe("source#0", &book(1, wall), old, wall);
        processed(&s, old, 1, false);
        assert_eq!(
            s.report()["ready"],
            false,
            "fresh source time must not hide an old local receipt"
        );
        let now = Instant::now();
        s.observe("source#0", &book(2, wall), now, wall);
        assert_eq!(
            s.report()["ready"],
            false,
            "received data alone is not publication evidence"
        );
        processed(&s, now, 1, false);
        assert_eq!(s.report()["ready"], true);
        s.retrying("source#0", "upstream_error");
        assert_eq!(s.report()["ready"], false);
        s.starting("source#0");
        processed(&s, now, 1, false);
        assert_eq!(
            s.report()["ready"],
            false,
            "old attempt cannot prime a new connection"
        );
        s.observe("source#0", &book(3, wall), now, wall);
        processed(&s, now, 2, false);
        assert_eq!(s.report()["ready"], true);
        s.stopping();
        assert_eq!(s.report()["ready"], false);
    }
    #[test]
    fn required_streams_and_unknown_future_time_do_not_silently_pass() {
        let s = setup();
        let wall = now_ms();
        let at = Instant::now();
        let mut config = crate::config::runtime::ReadinessConfig::default();
        config
            .required_streams
            .insert("binance:trade:perp:BTCUSDT".into(), 1000);
        s.configure(config, 16);
        s.observe("source#0", &book(1, wall), at, wall);
        processed(&s, at, 1, false);
        assert_eq!(s.report()["ready"], false);
        s.observe("source#0", &trade(5, wall), at, wall);
        processed(&s, at, 1, true);
        assert_eq!(s.report()["ready"], true);
        assert_eq!(
            s.observe("source#0", &trade(6, wall + 10_000), at, wall),
            Admission::Quarantine
        );
        assert_eq!(s.report()["ready"], false);
    }
    #[test]
    fn restored_native_checkpoint_continues_without_resetting_to_a_new_baseline() {
        let s = setup();
        let wall = now_ms();
        let at = Instant::now();
        s.install_checkpoints(vec![native_checkpoint(&trade(42, wall)).unwrap()])
            .unwrap();
        assert_eq!(
            s.observe("source#0", &trade(42, wall), at, wall),
            Admission::Duplicate
        );
        assert_eq!(
            s.observe("source#0", &trade(43, wall), at, wall),
            Admission::Accept(1)
        );
        assert!(!s.integrity().has_known_loss);
        assert_eq!(
            s.observe("source#0", &trade(45, wall), at, wall),
            Admission::Accept(1)
        );
        assert_eq!(s.integrity().reasons["native_sequence_gap"], 1);
    }

    #[test]
    fn quiet_trade_window_does_not_look_like_a_stuck_consumer_when_quotes_continue() {
        let s = setup();
        let wall = now_ms();
        let old = Instant::now() - std::time::Duration::from_secs(61);
        s.observe("source#0", &trade(1, wall - 61_000), old, wall - 61_000);
        processed(&s, old, 1, true);
        let now = Instant::now();
        s.observe("source#0", &book(10, wall), now, wall);
        processed(&s, now, 1, false);
        assert_eq!(
            s.report()["ready"],
            true,
            "consumer progress is relative to accepted trade work, not a universal trade frequency"
        );
    }
}
