use std::sync::Arc;

use tokio::sync::mpsc;

use crate::event_bus::EventBus;
use crate::journal::JournalMessage;
use crate::metrics::AppMetrics;
use crate::types::DataEvent;

pub struct EventRouter {
    source_rx: mpsc::Receiver<DataEvent>,
    agg_tx: mpsc::Sender<Arc<DataEvent>>,
    bus: EventBus,
    metrics: std::sync::Arc<AppMetrics>,
    bus_queue_capacity: usize,
    journal_tx: Option<mpsc::Sender<JournalMessage>>,
    outbox: Option<crate::outbox::Outbox>,
}

impl EventRouter {
    pub fn new(
        source_rx: mpsc::Receiver<DataEvent>,
        agg_tx: mpsc::Sender<Arc<DataEvent>>,
        bus: EventBus,
        metrics: std::sync::Arc<AppMetrics>,
        bus_queue_capacity: usize,
    ) -> Self {
        Self {
            source_rx,
            agg_tx,
            bus,
            metrics,
            bus_queue_capacity,
            journal_tx: None,
            outbox: None,
        }
    }

    pub fn with_journal(mut self, sender: Option<mpsc::Sender<JournalMessage>>) -> Self {
        self.journal_tx = sender;
        self
    }

    pub fn with_outbox(mut self, outbox: Option<crate::outbox::Outbox>) -> Self {
        self.outbox = outbox;
        self
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let (bus_tx, mut bus_rx) = mpsc::channel::<(
            Arc<DataEvent>,
            Option<crate::outbox::EventIdentity>,
        )>(self.bus_queue_capacity);
        let bus = self.bus.clone();
        let metrics = self.metrics.clone();
        let bus_task = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
            while let Some((event, identity)) = bus_rx.recv().await {
                bus.publish_identified_event(event.clone(), identity);
                metrics
                    .bus_events_published_total
                    .with_label_values(&[event_type(event.as_ref())])
                    .inc();
                if matches!(event.as_ref(), DataEvent::Tick(_)) {
                    metrics.bus_publish_total.inc();
                }
            }
        }));
        while let Some(event) = self.source_rx.recv().await {
            let mut batch = vec![(Arc::new(event), crate::types::now_ms())];
            while batch.len() < 64 {
                match self.source_rx.try_recv() {
                    Ok(event) => batch.push((Arc::new(event), crate::types::now_ms())),
                    Err(_) => break,
                }
            }
            for (event, _) in &batch {
                self.metrics
                    .events_ingested_total
                    .with_label_values(&[event_type(event)])
                    .inc();
                if matches!(event.as_ref(), DataEvent::Tick(_)) {
                    self.metrics.ticks_ingested_total.inc();
                }
            }
            // Durable acceptance is the transaction commit, before journal/bus
            // publication. A crash after commit is recovered by the sink.
            let identities = if let Some(outbox) = &self.outbox {
                let ids = outbox.append(batch.clone()).await?;
                self.metrics.outbox_committed_total.inc_by(ids.len() as u64);
                ids.into_iter().map(Some).collect::<Vec<_>>()
            } else {
                (0..batch.len())
                    .map(|_| Some(crate::outbox::EventIdentity::local()))
                    .collect()
            };
            for ((event, received_at_ms), identity) in batch.into_iter().zip(identities) {
                if let Some(tx) = &self.journal_tx {
                    tx.send(JournalMessage {
                        event: event.clone(),
                        received_at_ms,
                        identity: identity.clone(),
                    })
                    .await
                    .map_err(|_| anyhow::anyhow!("journal stopped before router drain"))?;
                }
                bus_tx
                    .send((event.clone(), identity))
                    .await
                    .map_err(|_| anyhow::anyhow!("bus publisher stopped"))?;
                self.agg_tx
                    .send(event)
                    .await
                    .map_err(|_| anyhow::anyhow!("aggregator stopped"))?;
            }
        }
        drop(bus_tx);
        bus_task.await?;
        Ok(())
    }
}

fn event_type(event: &DataEvent) -> &'static str {
    match event {
        DataEvent::Tick(_) => "tick",
        DataEvent::FundingRate(_) => "funding_rate",
        DataEvent::OpenInterest(_) => "open_interest",
        DataEvent::Trade(_) => "trade",
        DataEvent::Liquidation(_) => "liquidation",
        DataEvent::OrderBook(_) => "order_book",
        DataEvent::ExternalSignal(_) => "external_signal",
        DataEvent::Heartbeat { .. } => "heartbeat",
    }
}

#[cfg(test)]
mod tests {
    use super::EventRouter;
    use crate::event_bus::EventBus;
    use crate::metrics::AppMetrics;
    use crate::types::{DataEvent, FundingRateTick, MarketKind, MarketTick, now_ms};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn router_publishes_ticks_to_bus_and_aggregator() {
        let (source_tx, source_rx) = mpsc::channel(4);
        let (agg_tx, mut agg_rx) = mpsc::channel(4);
        let bus = EventBus::new_sharded(16, 1_000, 1);
        let router = EventRouter::new(source_rx, agg_tx, bus.clone(), AppMetrics::new(), 4);
        let router_task = tokio::spawn(router.run());

        source_tx
            .send(DataEvent::Tick(MarketTick {
                exchange: "okx",
                market: MarketKind::Spot,
                symbol: "BTCUSDT".into(),
                bid: 1.0,
                ask: 2.0,
                mark: None,
                funding_rate: None,
                ts_ms: now_ms(),
            }))
            .await
            .expect("source channel should accept test tick");
        drop(source_tx);

        assert!(matches!(
            agg_rx.recv().await.as_deref(),
            Some(DataEvent::Tick(_))
        ));
        router_task
            .await
            .expect("router task should finish")
            .expect("router must drain");
        assert_eq!(bus.snapshot_all().await.len(), 1);
    }

    #[tokio::test]
    async fn router_counts_non_tick_events() {
        let (source_tx, source_rx) = mpsc::channel(4);
        let (agg_tx, mut agg_rx) = mpsc::channel(4);
        let bus = EventBus::new_sharded(16, 1_000, 1);
        let metrics = AppMetrics::new();
        let router = EventRouter::new(source_rx, agg_tx, bus, metrics.clone(), 4);
        let router_task = tokio::spawn(router.run());

        source_tx
            .send(DataEvent::FundingRate(FundingRateTick {
                exchange: "binance",
                symbol: "BTCUSDT".into(),
                funding_rate: 0.01,
                next_funding_time_ms: None,
                mark_price: None,
                index_price: None,
                ts_ms: now_ms(),
            }))
            .await
            .expect("source channel should accept funding tick");
        drop(source_tx);

        assert!(matches!(
            agg_rx.recv().await.as_deref(),
            Some(DataEvent::FundingRate(_))
        ));
        router_task
            .await
            .expect("router task should finish")
            .expect("router must drain");
        assert!(
            metrics
                .render()
                .contains("events_ingested_total{event_type=\"funding_rate\"} 1")
        );
        assert!(
            metrics
                .render()
                .contains("bus_events_published_total{event_type=\"funding_rate\"} 1")
        );
    }

    #[tokio::test]
    async fn publication_follows_outbox_commit_and_carries_persistent_identity() {
        let root = std::env::temp_dir().join(format!(
            "marketbridge-router-outbox-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let box_ = crate::outbox::Outbox::open(
            &root.join("events.sqlite"),
            "ticks",
            1024 * 1024,
            4 * 1024 * 1024,
        )
        .unwrap();
        let (tx, rx) = mpsc::channel(4);
        let (agg_tx, mut agg_rx) = mpsc::channel(4);
        let bus = EventBus::new_sharded(16, 1000, 1);
        let mut subscriber = bus.subscribe_events();
        let task = tokio::spawn(
            EventRouter::new(rx, agg_tx, bus, AppMetrics::new(), 4)
                .with_outbox(Some(box_.clone()))
                .run(),
        );
        tx.send(DataEvent::Heartbeat {
            exchange: "test",
            ts_ms: 1,
        })
        .await
        .unwrap();
        drop(tx);
        assert!(agg_rx.recv().await.is_some());
        task.await.unwrap().unwrap();
        let published = subscriber.recv().await.unwrap();
        let rows = box_.pending(10).await.unwrap();
        assert_eq!(published.identity.as_ref(), Some(&rows[0].identity));
        let payload: serde_json::Value = serde_json::from_str(&published.json()).unwrap();
        assert_eq!(
            payload["event_identity"]["origin"],
            rows[0].identity.origin.as_ref()
        );
        assert_eq!(payload["type"], "heartbeat");
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn failed_outbox_commit_publishes_nothing() {
        let root = std::env::temp_dir().join(format!(
            "marketbridge-router-quota-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let box_ =
            crate::outbox::Outbox::open(&root.join("events.sqlite"), "ticks", 1, 4 * 1024 * 1024)
                .unwrap();
        let (tx, rx) = mpsc::channel(1);
        let (agg_tx, mut agg_rx) = mpsc::channel(1);
        let bus = EventBus::new_sharded(16, 1000, 1);
        let mut subscriber = bus.subscribe_events();
        let task = tokio::spawn(
            EventRouter::new(rx, agg_tx, bus, AppMetrics::new(), 1)
                .with_outbox(Some(box_.clone()))
                .run(),
        );
        tx.send(DataEvent::Heartbeat {
            exchange: "test",
            ts_ms: 1,
        })
        .await
        .unwrap();
        drop(tx);
        assert!(task.await.unwrap().is_err());
        assert!(agg_rx.recv().await.is_none());
        assert!(subscriber.recv().await.is_err());
        assert_eq!(box_.status().await.unwrap().pending_records, 0);
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }
}
