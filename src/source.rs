use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::metrics::AppMetrics;
use crate::types::{BackpressureMode, DataEvent};
use tracing::warn;

#[derive(Clone)]
pub struct SourceContext {
    pub tx: mpsc::Sender<SourceMessage>,
    pub source_id: std::sync::Arc<str>,
    pub backpressure: BackpressureMode,
    pub metrics: std::sync::Arc<AppMetrics>,
}

pub struct SourceMessage {
    pub event: DataEvent,
    pub received_at_ms: u64,
    pub trace: crate::ingestion::EventTrace,
}

impl From<DataEvent> for SourceMessage {
    fn from(event: DataEvent) -> Self {
        Self {
            event,
            received_at_ms: crate::types::now_ms(),
            trace: crate::ingestion::EventTrace {
                received_at: std::time::Instant::now(),
                source_id: "unattributed".into(),
                attempt: 0,
                source_quality: Default::default(),
            },
        }
    }
}

struct AdmissionGuard {
    state: crate::ingestion::IngestionState,
    armed: bool,
}
impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        if self.armed {
            self.state.loss("admission_cancelled", 1);
        }
    }
}

impl SourceContext {
    pub async fn emit(&self, event: DataEvent) -> Result<()> {
        self.emit_at(event, crate::types::now_ms(), std::time::Instant::now())
            .await
    }

    pub async fn emit_at(
        &self,
        event: DataEvent,
        received_at_ms: u64,
        received_at: std::time::Instant,
    ) -> Result<()> {
        if !event.has_finite_numbers()
            || !event.has_valid_market_values()
            || !event.fits_ingestion_limits()
        {
            warn!("dropping data event with invalid numeric field or oversized variable payload");
            self.metrics.ticks_dropped_total.inc();
            self.metrics.ingestion.loss("invalid_numeric_or_payload", 1);
            return Ok(());
        }
        let mut guard = AdmissionGuard {
            state: self.metrics.ingestion.clone(),
            armed: true,
        };
        // Reserve capacity before advancing continuity. Cancellation cannot
        // create a native checkpoint for an event that never entered the queue.
        let permit = match self.backpressure {
            BackpressureMode::Block => match self.tx.reserve().await {
                Ok(permit) => permit,
                Err(error) => {
                    guard.armed = false;
                    self.metrics.ingestion.loss("pipeline_closed", 1);
                    return Err(error.into());
                }
            },
            BackpressureMode::DropNewest => match self.tx.try_reserve() {
                Ok(permit) => permit,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    guard.armed = false;
                    self.metrics.ticks_dropped_total.inc();
                    self.metrics.ingestion.loss("queue_full", 1);
                    return Ok(());
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    guard.armed = false;
                    self.metrics.ingestion.loss("pipeline_closed", 1);
                    anyhow::bail!("source pipeline closed");
                }
            },
        };
        let attempt = match self.metrics.ingestion.observe(
            &self.source_id,
            &event,
            received_at,
            received_at_ms,
        ) {
            crate::ingestion::Admission::Accept(attempt) => attempt,
            crate::ingestion::Admission::Duplicate => {
                guard.armed = false;
                return Ok(());
            }
            crate::ingestion::Admission::Quarantine => {
                guard.armed = false;
                self.metrics.ticks_dropped_total.inc();
                return Ok(());
            }
        };
        let domain = crate::ingestion::domain(&event);
        permit.send(SourceMessage {
            event,
            received_at_ms,
            trace: crate::ingestion::EventTrace {
                received_at,
                source_id: self.source_id.clone(),
                attempt,
                source_quality: self.metrics.ingestion.integrity(),
            },
        });
        self.metrics
            .ingestion
            .accepted(&self.source_id, received_at, attempt, domain);
        guard.armed = false;
        Ok(())
    }
}

#[async_trait]
pub trait ExchangeSource: Send + Sync {
    fn name(&self) -> &'static str;
    fn source_type(&self) -> &'static str {
        ""
    }
    fn label(&self) -> String {
        let source_type = self.source_type();
        if source_type.is_empty() {
            self.name().to_string()
        } else {
            format!("{}/{}", self.name(), source_type)
        }
    }
    async fn run(&self, ctx: SourceContext) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[tokio::test]
    async fn closed_pipeline_is_not_silently_counted_as_congestion() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let metrics = AppMetrics::new();
        let ctx = SourceContext {
            tx,
            source_id: "test#0".into(),
            metrics: metrics.clone(),
            backpressure: BackpressureMode::DropNewest,
        };
        assert!(
            ctx.emit(DataEvent::Heartbeat {
                exchange: "test",
                ts_ms: 1
            })
            .await
            .is_err()
        );
        assert_eq!(metrics.ticks_dropped_total.get(), 0);
    }

    struct TypedSource;

    #[async_trait]
    impl ExchangeSource for TypedSource {
        fn name(&self) -> &'static str {
            "binance"
        }

        fn source_type(&self) -> &'static str {
            "funding"
        }

        async fn run(&self, _ctx: SourceContext) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn source_label_includes_source_type() {
        assert_eq!(TypedSource.label(), "binance/funding");
    }

    fn trade(id: u64) -> DataEvent {
        DataEvent::Trade(crate::types::TradeTick {
            exchange: "binance",
            market: crate::types::MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 1.0,
            side: crate::types::TradeSide::Buy,
            trade_id: Some(id.to_string().into()),
            ts_ms: crate::types::now_ms(),
        })
    }
    fn context(mode: BackpressureMode) -> (SourceContext, mpsc::Receiver<SourceMessage>) {
        let metrics = AppMetrics::new();
        metrics.ingestion.register("test#0", "test").unwrap();
        metrics.ingestion.starting("test#0");
        let (tx, rx) = mpsc::channel(1);
        (
            SourceContext {
                tx,
                source_id: "test#0".into(),
                backpressure: mode,
                metrics,
            },
            rx,
        )
    }
    #[tokio::test]
    async fn queue_loss_is_visible_and_does_not_advance_unaccepted_native_ids() {
        let (ctx, mut rx) = context(BackpressureMode::DropNewest);
        ctx.emit(trade(1)).await.unwrap();
        ctx.emit(trade(2)).await.unwrap();
        assert_eq!(
            ctx.metrics
                .ingestion
                .native_sequence("binance:trade:perp:BTCUSDT"),
            Some(1)
        );
        assert_eq!(ctx.metrics.ticks_dropped_total.get(), 1);
        rx.recv().await.unwrap();
        ctx.emit(trade(3)).await.unwrap();
        let message = rx.recv().await.unwrap();
        assert_eq!(message.trace.source_quality.reasons["queue_full"], 1);
        assert_eq!(
            message.trace.source_quality.reasons["native_sequence_gap"],
            1
        );
        assert!(message.trace.source_quality.has_known_loss);
    }
    #[tokio::test]
    async fn cancelled_backpressure_wait_is_visible_without_advancing_checkpoint() {
        let (ctx, mut rx) = context(BackpressureMode::Block);
        ctx.emit(trade(1)).await.unwrap();
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let notify = started.clone();
        let worker = ctx.clone();
        let task = tokio::spawn(async move {
            notify.notify_one();
            worker.emit(trade(2)).await
        });
        started.notified().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(
            ctx.metrics
                .ingestion
                .native_sequence("binance:trade:perp:BTCUSDT"),
            Some(1)
        );
        assert_eq!(
            ctx.metrics.ingestion.integrity().reasons["admission_cancelled"],
            1
        );
        rx.recv().await.unwrap();
        ctx.emit(trade(2)).await.unwrap();
        assert!(rx.recv().await.is_some());
    }
    #[tokio::test]
    async fn exact_native_duplicates_never_reach_router_twice() {
        let (ctx, mut rx) = context(BackpressureMode::Block);
        let event = trade(1);
        ctx.emit(event.clone()).await.unwrap();
        rx.recv().await.unwrap();
        ctx.emit(event).await.unwrap();
        assert!(rx.try_recv().is_err());
        assert!(!ctx.metrics.ingestion.integrity().has_known_loss);
        let mut invalid = trade(2);
        if let DataEvent::Trade(t) = &mut invalid {
            t.price = -1.0;
            t.qty = -1.0;
        }
        ctx.emit(invalid).await.unwrap();
        assert!(rx.try_recv().is_err());
        assert!(
            ctx.metrics.ingestion.integrity().has_known_loss,
            "two negative values must not become a valid positive notional"
        );
    }
}
