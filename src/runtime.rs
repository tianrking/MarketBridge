use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::metrics::AppMetrics;
use crate::source::{ExchangeSource, SourceContext, SourceMessage};
use crate::types::BackpressureMode;

pub struct RuntimeHandle {
    pub rx: mpsc::Receiver<SourceMessage>,
    pub shutdown: CancellationToken,
    pub tasks: Vec<JoinHandle<()>>,
}

pub struct SourceRuntime {
    pub queue_capacity: usize,
    pub backpressure: BackpressureMode,
    pub max_backoff: Duration,
    pub metrics: Arc<AppMetrics>,
}

impl SourceRuntime {
    pub fn new(
        queue_capacity: usize,
        backpressure: BackpressureMode,
        metrics: Arc<AppMetrics>,
    ) -> Self {
        Self {
            queue_capacity,
            backpressure,
            max_backoff: Duration::from_secs(30),
            metrics,
        }
    }

    pub fn spawn_sources(
        &self,
        sources: Vec<Arc<dyn ExchangeSource>>,
    ) -> anyhow::Result<RuntimeHandle> {
        anyhow::ensure!(
            sources.len() <= 1024,
            "configured collectors exceed readiness source budget"
        );
        let (tx, rx) = mpsc::channel(self.queue_capacity);
        let shutdown = CancellationToken::new();
        let mut tasks = Vec::with_capacity(sources.len());

        for (index, source) in sources.into_iter().enumerate() {
            let id: Arc<str> = format!("{}#{index}", source.label()).into();
            self.metrics.ingestion.register(&id, &source.label())?;
            let ctx = SourceContext {
                source_id: id.clone(),
                tx: tx.clone(),
                backpressure: self.backpressure,
                metrics: self.metrics.clone(),
            };
            let max_backoff = self.max_backoff;
            let stop = shutdown.clone();
            let source = source.clone();
            let task = tokio::spawn(async move {
                let mut delay = Duration::from_secs(1);
                let source_name = source.name();
                let source_label = source.label();
                loop {
                    if stop.is_cancelled() {
                        break;
                    }

                    ctx.metrics.ingestion.starting(&id);
                    info!(exchange = source_name, source = %source_label, "source start");
                    let run_ctx = ctx.clone();
                    let source_run = source.clone();
                    let mut run_task = tokio::spawn(async move { source_run.run(run_ctx).await });

                    let ran_ok = tokio::select! {
                        _ = stop.cancelled() => {
                            run_task.abort();
                            let _ = run_task.await;
                            break;
                        }
                        join = &mut run_task => {
                            match join {
                                Ok(Ok(())) => {
                                    ctx.metrics.ingestion.retrying(&id,"source returned; reconnecting");
                                    warn!(exchange = source_name, source = %source_label, "source exited normally, reconnecting");
                                    true
                                }
                                Ok(Err(e)) => {
                                    ctx.metrics.ingestion.retrying(&id,"source_error; see protected logs");
                                    error!(exchange = source_name, source = %source_label, error = %e, "source error");
                                    false
                                }
                                Err(e) => {
                                    ctx.metrics.ingestion.retrying(&id,"source_task_panic; see protected logs");
                                    error!(exchange = source_name, source = %source_label, error = %e, "source task panicked");
                                    false
                                }
                            }
                        }
                    };

                    if ran_ok {
                        delay = Duration::from_secs(1);
                    }

                    tokio::select! {
                        _ = stop.cancelled() => break,
                        _ = sleep(delay) => {}
                    }

                    delay = (delay * 2).min(max_backoff);
                }
            });
            tasks.push(task);
        }

        // A research-only configuration may have zero collectors. Keep the
        // pipeline/API alive until cancellation, not merely until the last
        // source sender is dropped.
        let idle_stop = shutdown.clone();
        tasks.push(tokio::spawn(async move {
            idle_stop.cancelled().await;
            drop(tx);
        }));

        Ok(RuntimeHandle {
            rx,
            shutdown,
            tasks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn research_only_runtime_stays_open_until_shutdown() {
        let runtime = SourceRuntime::new(4, BackpressureMode::Block, AppMetrics::new());
        let mut handle = runtime.spawn_sources(vec![]).unwrap();
        assert!(matches!(
            handle.rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        handle.shutdown.cancel();
        for task in handle.tasks {
            task.await.unwrap();
        }
        assert!(handle.rx.recv().await.is_none());
    }
}
