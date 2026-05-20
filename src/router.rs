use tokio::sync::mpsc;

use crate::event_bus::EventBus;
use crate::metrics::AppMetrics;
use crate::types::DataEvent;

pub struct EventRouter {
    source_rx: mpsc::Receiver<DataEvent>,
    agg_tx: mpsc::Sender<DataEvent>,
    bus: EventBus,
    metrics: std::sync::Arc<AppMetrics>,
}

#[cfg(test)]
mod tests {
    use super::EventRouter;
    use crate::event_bus::EventBus;
    use crate::metrics::AppMetrics;
    use crate::types::{DataEvent, MarketKind, MarketTick, now_ms};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn router_publishes_ticks_to_bus_and_aggregator() {
        let (source_tx, source_rx) = mpsc::channel(4);
        let (agg_tx, mut agg_rx) = mpsc::channel(4);
        let bus = EventBus::new(16, 1_000);
        let router = EventRouter::new(source_rx, agg_tx, bus.clone(), AppMetrics::new());
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

        assert!(matches!(agg_rx.recv().await, Some(DataEvent::Tick(_))));
        router_task.await.expect("router task should finish");
        assert_eq!(bus.snapshot_all().await.len(), 1);
    }
}

impl EventRouter {
    pub fn new(
        source_rx: mpsc::Receiver<DataEvent>,
        agg_tx: mpsc::Sender<DataEvent>,
        bus: EventBus,
        metrics: std::sync::Arc<AppMetrics>,
    ) -> Self {
        Self {
            source_rx,
            agg_tx,
            bus,
            metrics,
        }
    }

    pub async fn run(mut self) {
        let (bus_tx, mut bus_rx) = mpsc::channel::<DataEvent>(1024);
        let bus = self.bus.clone();
        let metrics = self.metrics.clone();
        let bus_task = tokio::spawn(async move {
            while let Some(event) = bus_rx.recv().await {
                bus.publish_from_event(&event);
                if matches!(&event, DataEvent::Tick(_)) {
                    metrics.bus_publish_total.inc();
                }
            }
        });

        while let Some(event) = self.source_rx.recv().await {
            if matches!(&event, DataEvent::Tick(_)) {
                self.metrics.ticks_ingested_total.inc();
            }
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
