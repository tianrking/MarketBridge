use std::sync::Arc;

use tokio::sync::mpsc;

use crate::event_bus::EventBus;
use crate::metrics::AppMetrics;
use crate::types::DataEvent;

pub struct EventRouter {
    source_rx: mpsc::Receiver<DataEvent>,
    agg_tx: mpsc::Sender<Arc<DataEvent>>,
    bus: EventBus,
    metrics: std::sync::Arc<AppMetrics>,
    bus_queue_capacity: usize,
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
        }
    }

    pub async fn run(mut self) {
        let (bus_tx, mut bus_rx) = mpsc::channel::<Arc<DataEvent>>(self.bus_queue_capacity);
        let bus = self.bus.clone();
        let metrics = self.metrics.clone();
        let bus_task = tokio::spawn(async move {
            while let Some(event) = bus_rx.recv().await {
                bus.publish_shared_event(event.clone());
                metrics
                    .bus_events_published_total
                    .with_label_values(&[event_type(event.as_ref())])
                    .inc();
                if matches!(event.as_ref(), DataEvent::Tick(_)) {
                    metrics.bus_publish_total.inc();
                }
            }
        });

        while let Some(event) = self.source_rx.recv().await {
            self.metrics
                .events_ingested_total
                .with_label_values(&[event_type(&event)])
                .inc();
            if matches!(&event, DataEvent::Tick(_)) {
                self.metrics.ticks_ingested_total.inc();
            }
            let event = Arc::new(event);
            if bus_tx.send(event.clone()).await.is_err() {
                break;
            }
            if self.agg_tx.send(event).await.is_err() {
                break;
            }
        }
        drop(bus_tx);
        let _ = bus_task.await;
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
        router_task.await.expect("router task should finish");
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
        router_task.await.expect("router task should finish");
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
}
