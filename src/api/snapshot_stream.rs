use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::deribit_cache::{DeribitOptionCache, DeribitOptionFilter};
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::domains::prediction::book::envelope_from_polymarket_book;
use crate::polymarket_ws::PolymarketBookCache;

const SNAPSHOT_STREAM_INTERVAL_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotStreamDomain {
    OptionsChain,
    PredictionBook,
}

#[derive(Debug, Clone)]
pub struct SharedSnapshot {
    pub domain: SnapshotStreamDomain,
    pub source: String,
    pub symbol: Option<String>,
    pub product_type: &'static str,
    pub stale: bool,
    pub json: Arc<str>,
}

#[derive(Clone)]
pub struct SnapshotStreamHub {
    options_tx: broadcast::Sender<Arc<SharedSnapshot>>,
    prediction_tx: broadcast::Sender<Arc<SharedSnapshot>>,
}

impl SnapshotStreamHub {
    pub fn new(capacity: usize) -> Self {
        let (options_tx, _) = broadcast::channel(capacity);
        let (prediction_tx, _) = broadcast::channel(capacity);
        Self {
            options_tx,
            prediction_tx,
        }
    }

    pub fn subscribe_options(&self) -> broadcast::Receiver<Arc<SharedSnapshot>> {
        self.options_tx.subscribe()
    }

    pub fn subscribe_prediction(&self) -> broadcast::Receiver<Arc<SharedSnapshot>> {
        self.prediction_tx.subscribe()
    }

    pub fn spawn(
        &self,
        deribit_cache: DeribitOptionCache,
        polymarket_cache: PolymarketBookCache,
        shutdown: CancellationToken,
    ) -> JoinHandle<()> {
        let hub = self.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_millis(SNAPSHOT_STREAM_INTERVAL_MS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        hub.publish_options(&deribit_cache).await;
                        hub.publish_predictions(&polymarket_cache).await;
                    }
                }
            }
        })
    }

    async fn publish_options(&self, cache: &DeribitOptionCache) {
        let rows = cache
            .filtered(DeribitOptionFilter {
                include_stale: true,
                ..Default::default()
            })
            .await;
        for row in rows {
            let envelope = envelope_from_deribit_summary(row);
            let snapshot = SharedSnapshot {
                domain: SnapshotStreamDomain::OptionsChain,
                source: envelope.source_ref.source.clone(),
                symbol: envelope.instrument_ref.symbol.clone(),
                product_type: "option",
                stale: envelope.freshness.stale,
                json: match serde_json::to_string(&envelope) {
                    Ok(json) => json.into(),
                    Err(error) => {
                        warn!(%error, "options snapshot serialization failed");
                        continue;
                    }
                },
            };
            let _ = self.options_tx.send(Arc::new(snapshot));
        }
    }

    async fn publish_predictions(&self, cache: &PolymarketBookCache) {
        for row in cache.all().await {
            let envelope = envelope_from_polymarket_book(row);
            let snapshot = SharedSnapshot {
                domain: SnapshotStreamDomain::PredictionBook,
                source: envelope.source_ref.source.clone(),
                symbol: envelope.instrument_ref.symbol.clone(),
                product_type: "binary_outcome",
                stale: envelope.freshness.stale,
                json: match serde_json::to_string(&envelope) {
                    Ok(json) => json.into(),
                    Err(error) => {
                        warn!(%error, "prediction snapshot serialization failed");
                        continue;
                    }
                },
            };
            let _ = self.prediction_tx.send(Arc::new(snapshot));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_exposes_independent_snapshot_channels() {
        let hub = SnapshotStreamHub::new(8);
        let _options = hub.subscribe_options();
        let _prediction = hub.subscribe_prediction();
    }
}
