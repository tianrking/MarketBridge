use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::metrics::AppMetrics;
use crate::types::{BackpressureMode, DataEvent};
use tracing::warn;

#[derive(Clone)]
pub struct SourceContext {
    pub tx: mpsc::Sender<DataEvent>,
    pub backpressure: BackpressureMode,
    pub metrics: std::sync::Arc<AppMetrics>,
}

impl SourceContext {
    pub async fn emit(&self, ev: DataEvent) -> Result<()> {
        if !ev.has_finite_numbers() {
            warn!("dropping data event with non-finite numeric field");
            self.metrics.ticks_dropped_total.inc();
            return Ok(());
        }
        match self.backpressure {
            BackpressureMode::Block => {
                self.tx.send(ev).await?;
            }
            BackpressureMode::DropNewest => {
                if self.tx.try_send(ev).is_err() {
                    self.metrics.ticks_dropped_total.inc();
                }
            }
        }
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
}
