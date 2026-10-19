mod aggregator;
mod aggregator_signal;
mod api;
mod asset_registry;
mod catalog;
mod clickhouse_sink;
mod config;
mod connectors;
mod core;
mod data_lake;
mod data_quality;
mod deribit_cache;
mod domains;
mod event_bus;
mod event_snapshots;
mod journal;
mod journal_replay;
mod klines;
mod load_test;
mod market_discovery;
mod metrics;
mod onchain;
mod opportunity_scan;
mod order_flow;
mod outbox;
mod paper;
mod polymarket_ws;
mod redis_sink;
mod relative_value;
mod research_control;
mod research_engine;
mod research_events;
mod research_lab;
mod research_portfolio;
mod research_store;
mod router;
mod runtime;
mod source;
mod source_roadmap;
mod squeeze_radar;
mod strategy_state;
mod supply;
mod types;
mod venue_status;

pub const BUILD_REVISION: &str = match option_env!("MARKETBRIDGE_BUILD_REVISION") {
    Some(revision) => revision,
    None => "unattested-build",
};

use aggregator::SpreadAggregator;
use anyhow::Context;
use api::snapshot_stream::SnapshotStreamHub;
use api::streaming::set_ws_send_timeout_ms;
use api::{ApiState, build_router};
use clickhouse_sink::spawn_clickhouse_sink;
use config::AppConfig;
use connectors::cex::registry::build_sources;
use data_lake::DataLakeStore;
use deribit_cache::{
    DeribitOptionCache, spawn_binance_option_cache, spawn_binance_option_ws_cache,
    spawn_bybit_option_cache, spawn_bybit_option_ws_cache, spawn_deribit_option_cache,
    spawn_deribit_option_ws_cache, spawn_okx_option_cache, spawn_okx_option_ws_cache,
};
use event_bus::EventBus;
use futures_util::{StreamExt, stream::FuturesUnordered};
use klines::{KlineStore, spawn_kline_service};
use metrics::AppMetrics;
use onchain::{OnchainTransferStore, log_onchain_start, spawn_onchain_collectors};
use order_flow::{OrderFlowStore, spawn_order_flow_service};
use polymarket_ws::{PolymarketBookCache, spawn_polymarket_ws_cache};
use redis_sink::spawn_redis_sink;
use router::EventRouter;
use runtime::SourceRuntime;
use strategy_state::{StrategyStateStore, spawn_strategy_state_service};
use supply::{SupplyReferenceService, SupplySnapshotStore};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use types::DataEvent;
use venue_status::VenueAssetStatusStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--outbox") {
        let action = args
            .get(2)
            .context("--outbox requires status, export or replay")?;
        let path = args.get(3).context("--outbox requires a database path")?;
        let config = AppConfig::load()?;
        let limits = &config.runtime.outbox;
        if action == "status" {
            anyhow::ensure!(args.len() == 4, "unexpected outbox arguments");
        } else {
            anyhow::ensure!(
                matches!(action.as_str(), "export" | "replay") && args.len() == 5,
                "outbox export/replay requires one file path"
            );
        }
        anyhow::ensure!(
            std::path::Path::new(path).is_file(),
            "outbox database does not exist"
        );
        let box_ = outbox::Outbox::open(
            std::path::Path::new(path),
            &config.runtime.redis_stream_prefix,
            limits.max_pending_bytes,
            limits.max_database_bytes,
        )?;
        let result = match action.as_str() {
            "status" => serde_json::to_value(box_.status().await?)?,
            "export" => {
                serde_json::json!({"exported_records":box_.export(std::path::Path::new(&args[4])).await?})
            }
            _ => {
                serde_json::json!({"replayed_records":box_.replay(std::path::Path::new(&args[4])).await?})
            }
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--replay-journal") {
        let path = args.get(2).ok_or_else(|| {
            anyhow::anyhow!("--replay-journal requires a sealed journal and route JSON path")
        })?;
        let route_path = args
            .get(3)
            .ok_or_else(|| anyhow::anyhow!("route JSON path required"))?;
        let file = std::fs::File::open(route_path)?;
        anyhow::ensure!(
            file.metadata()?.len() <= 2 * 1024 * 1024,
            "route exceeds 2 MiB"
        );
        let report =
            journal_replay::replay(std::path::Path::new(path), serde_json::from_reader(file)?)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if matches!(
        args.get(1).map(String::as_str),
        Some("--evaluate" | "--replay" | "--paper" | "--scan")
    ) {
        let path = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("research command requires a JSON path"))?;
        let file = std::fs::File::open(path)?;
        anyhow::ensure!(
            file.metadata()?.len() <= 2 * 1024 * 1024,
            "research input exceeds 2 MiB"
        );
        let output = if args[1] == "--evaluate" {
            let request = serde_json::from_reader(file)?;
            serde_json::to_value(research_engine::scan(&request).map_err(anyhow::Error::msg)?)?
        } else if args[1] == "--scan" {
            let request = serde_json::from_reader(file)?;
            serde_json::to_value(opportunity_scan::evaluate(&request).map_err(anyhow::Error::msg)?)?
        } else if args[1] == "--paper" {
            let request = serde_json::from_reader(file)?;
            serde_json::to_value(paper::simulate(&request).map_err(anyhow::Error::msg)?)?
        } else {
            let request = serde_json::from_reader(file)?;
            serde_json::to_value(research_engine::replay(&request).map_err(anyhow::Error::msg)?)?
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--verify-journal") {
        let path = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("--verify-journal requires a path"))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&journal::verify(std::path::Path::new(path))?)?
        );
        return Ok(());
    }
    if let Some(load_test_cfg) = load_test::load_test_config_from_args(&args)? {
        load_test::run_load_test(load_test_cfg).await?;
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cfg = AppConfig::load()?;
    // Fail before starting collectors or opening a recording if the API cannot
    // be served. Supervisors must see an unsuccessful startup, not exit code 0.
    let api_listener = tokio::net::TcpListener::bind(&cfg.runtime.api_addr)
        .await
        .with_context(|| format!("failed to bind API listener: {}", cfg.runtime.api_addr))?;
    let shutdown_signal = shutdown_signal()?;
    tokio::pin!(shutdown_signal);
    let metrics = AppMetrics::new();
    let outbox = if cfg.runtime.outbox.enabled
        || (cfg.runtime.redis_url.is_some() && !cfg.runtime.outbox.legacy_redis)
    {
        let path = std::env::var_os("MARKETBRIDGE_OUTBOX_DB")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| cfg.runtime.outbox.path.clone().into());
        let prefix = cfg.runtime.redis_stream_prefix.clone();
        let limits = cfg.runtime.outbox.clone();
        Some(
            tokio::task::spawn_blocking(move || {
                outbox::Outbox::open(
                    &path,
                    &prefix,
                    limits.max_pending_bytes,
                    limits.max_database_bytes,
                )
            })
            .await??,
        )
    } else {
        None
    };
    let runtime = SourceRuntime::new(
        cfg.runtime.queue_capacity,
        cfg.backpressure_mode(),
        metrics.clone(),
    );
    let sources = build_sources(&cfg);

    let handle = runtime.spawn_sources(sources);
    let shutdown = handle.shutdown.clone();
    let tasks = handle.tasks;
    let (journal_tx, journal_task) = if let Some(path) = std::env::var_os("MARKETBRIDGE_RECORD_DIR")
    {
        let (tx, task) = journal::start(
            std::path::Path::new(&path),
            metrics.clone(),
            shutdown.clone(),
        )?;
        (Some(tx), Some(task))
    } else {
        (None, None)
    };

    let bus = EventBus::new_sharded(
        cfg.runtime.broadcast_capacity,
        cfg.runtime.stale_ttl_ms,
        cfg.runtime.event_bus_shards,
    );

    let deribit_cache = DeribitOptionCache::new(cfg.deribit.stale_ttl_ms);
    let polymarket_cache = PolymarketBookCache::new(cfg.polymarket.stale_ttl_ms);
    let snapshot_stream_hub = SnapshotStreamHub::new(cfg.runtime.broadcast_capacity);
    let kline_store = KlineStore::new(cfg.klines.sqlite_path.clone());
    let data_lake_store =
        DataLakeStore::new(cfg.klines.lake_root.clone(), cfg.klines.sqlite_path.clone());
    if let Err(error) = data_lake_store.init().await {
        error!(%error, "data lake manifest init failed");
    }
    set_ws_send_timeout_ms(cfg.runtime.ws_send_timeout_ms);

    let order_flow_store = OrderFlowStore::with_limits(
        cfg.runtime.order_flow_large_trade_notional_usdt,
        cfg.runtime.memory.clone(),
    );
    let onchain_store = OnchainTransferStore::default();
    let strategy_state_store = StrategyStateStore::with_limits(cfg.runtime.memory.clone());
    let supply_store = SupplySnapshotStore::default();
    let venue_status_store = VenueAssetStatusStore::default();
    let source_catalog = catalog::source_catalog_for_config(&cfg);
    let http = reqwest::Client::new();
    let supply_service = SupplyReferenceService::new(
        cfg.reference_data.supply.clone(),
        supply_store.clone(),
        http.clone(),
    );

    let research_store = research_store::ResearchStore::open(std::path::Path::new(
        &std::env::var("MARKETBRIDGE_RESEARCH_DB")
            .unwrap_or_else(|_| "data/research-workspace.sqlite".into()),
    ))?;
    let research_control = research_control::ResearchControl::new(research_store.clone())?;
    let research_control_task = research_control.spawn(bus.clone(), shutdown.clone());
    let announcement_task = research_events::spawn_ingestion(
        bus.clone(),
        research_control.clone(),
        research_store.clone(),
        shutdown.clone(),
    );
    let api_router = build_router(ApiState {
        outbox: outbox.clone(),
        research_store,
        research_control,
        source_catalog,
        bus: bus.clone(),
        metrics: metrics.clone(),
        http: http.clone(),
        deribit_cache: deribit_cache.clone(),
        polymarket_cache: polymarket_cache.clone(),
        kline_store: kline_store.clone(),
        data_lake_store: data_lake_store.clone(),
        order_flow_store: order_flow_store.clone(),
        onchain_store: onchain_store.clone(),
        strategy_state_store: strategy_state_store.clone(),
        supply_store: supply_store.clone(),
        supply_service,
        venue_status_store,
        snapshot_stream_hub: snapshot_stream_hub.clone(),
        api_access_guard: ApiState::api_access_guard_from_runtime(&cfg.runtime),
        api_cors: ApiState::api_cors_from_runtime(&cfg.runtime),
    });
    let api_addr = cfg.runtime.api_addr.clone();
    let api_shutdown = shutdown.clone();
    let mut api_task = tokio::spawn(async move {
        info!(addr=%api_addr, "api server started");
        if let Err(e) = axum::serve(api_listener, api_router)
            .with_graceful_shutdown(async move {
                api_shutdown.cancelled().await;
            })
            .await
        {
            error!(error=%e, "api server failed");
        }
    });

    let redis_task = cfg.runtime.redis_url.clone().map(|url| {
        if let Some(outbox) = outbox.clone() {
            redis_sink::spawn_outbox_sink(
                outbox,
                url,
                cfg.runtime.outbox.require_redis_aof,
                metrics.clone(),
                shutdown.clone(),
            )
        } else {
            spawn_redis_sink(
                bus.clone(),
                url,
                cfg.runtime.redis_stream_prefix.clone(),
                cfg.runtime.redis_dead_letter_path.clone(),
                metrics.clone(),
                shutdown.clone(),
            )
        }
    });

    let clickhouse_task = cfg.runtime.clickhouse.enabled.then(|| {
        spawn_clickhouse_sink(
            bus.clone(),
            cfg.runtime.clickhouse.clone(),
            shutdown.clone(),
        )
    });

    let deribit_task = cfg.deribit.enabled.then(|| {
        spawn_deribit_option_cache(
            cfg.deribit.clone(),
            http.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });
    let deribit_ws_task = cfg.deribit.enabled.then(|| {
        spawn_deribit_option_ws_cache(
            cfg.deribit.clone(),
            http.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });

    let okx_options_task = cfg.okx_options.enabled.then(|| {
        spawn_okx_option_cache(
            cfg.okx_options.clone(),
            http.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });
    let okx_options_ws_task = cfg.okx_options.enabled.then(|| {
        spawn_okx_option_ws_cache(
            cfg.okx_options.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });

    let bybit_options_task = cfg.bybit_options.enabled.then(|| {
        spawn_bybit_option_cache(
            cfg.bybit_options.clone(),
            http.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });
    let bybit_options_ws_task = cfg.bybit_options.enabled.then(|| {
        spawn_bybit_option_ws_cache(
            cfg.bybit_options.clone(),
            http.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });

    let binance_options_task = cfg.binance_options.enabled.then(|| {
        spawn_binance_option_cache(
            cfg.binance_options.clone(),
            http.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });
    let binance_options_ws_task = cfg.binance_options.enabled.then(|| {
        spawn_binance_option_ws_cache(
            cfg.binance_options.clone(),
            http.clone(),
            deribit_cache.clone(),
            shutdown.clone(),
        )
    });

    let polymarket_task = cfg.polymarket.enabled.then(|| {
        spawn_polymarket_ws_cache(
            cfg.polymarket.clone(),
            http.clone(),
            polymarket_cache.clone(),
            shutdown.clone(),
        )
    });

    let snapshot_stream_task = snapshot_stream_hub.spawn(
        deribit_cache.clone(),
        polymarket_cache.clone(),
        shutdown.clone(),
    );

    let kline_task = cfg.klines.enabled.then(|| {
        spawn_kline_service(
            cfg.klines.clone(),
            cfg.clone(),
            http.clone(),
            bus.clone(),
            kline_store.clone(),
            shutdown.clone(),
        )
    });

    let order_flow_task =
        spawn_order_flow_service(bus.clone(), order_flow_store.clone(), shutdown.clone());
    let strategy_state_task =
        spawn_strategy_state_service(bus.clone(), strategy_state_store.clone(), shutdown.clone());
    log_onchain_start(&cfg.onchain);
    let onchain_tasks = spawn_onchain_collectors(
        cfg.onchain.clone(),
        http.clone(),
        onchain_store.clone(),
        shutdown.clone(),
    );

    let (agg_tx, agg_rx) = mpsc::channel::<std::sync::Arc<DataEvent>>(cfg.runtime.queue_capacity);
    let router = EventRouter::new(
        handle.rx,
        agg_tx,
        bus.clone(),
        metrics.clone(),
        cfg.runtime.router_publish_queue_capacity(),
    )
    .with_journal(journal_tx)
    .with_outbox(outbox.clone());
    let router_task = tokio::spawn(router.run());

    let mut agg_task = tokio::spawn(SpreadAggregator::from_config(&cfg).run(agg_rx));

    let mut background_tasks = TaskGroup::default();
    background_tasks.required("research_control", research_control_task);
    background_tasks.required("research_announcements", announcement_task);
    background_tasks.optional_result("redis_sink", redis_task);
    background_tasks.optional("clickhouse_sink", clickhouse_task);
    background_tasks.optional("deribit_options", deribit_task);
    background_tasks.optional("deribit_options_ws", deribit_ws_task);
    background_tasks.optional("okx_options", okx_options_task);
    background_tasks.optional("okx_options_ws", okx_options_ws_task);
    background_tasks.optional("bybit_options", bybit_options_task);
    background_tasks.optional("bybit_options_ws", bybit_options_ws_task);
    background_tasks.optional("binance_options", binance_options_task);
    background_tasks.optional("binance_options_ws", binance_options_ws_task);
    background_tasks.optional("polymarket_ws_cache", polymarket_task);
    background_tasks.optional("kline_service", kline_task);
    background_tasks.required("order_flow", order_flow_task);
    background_tasks.required("strategy_state", strategy_state_task);
    background_tasks.required("snapshot_stream", snapshot_stream_task);
    background_tasks.extend("onchain_collector", onchain_tasks);
    background_tasks.extend("source", tasks);
    background_tasks.optional_result("router", Some(router_task));
    background_tasks.optional_result("journal", journal_task);

    let mut agg_joined = false;
    let mut api_joined = false;
    let mut failure = tokio::select! {
        result = &mut shutdown_signal => {
            info!("shutdown signal received, stopping services");
            result.err().map(anyhow::Error::from)
        }
        res = &mut agg_task => {
            agg_joined = true;
            Some(unexpected_exit("aggregator", res.map_err(anyhow::Error::from)))
        }
        res = &mut api_task => {
            api_joined = true;
            Some(unexpected_exit("api", res.map_err(anyhow::Error::from)))
        }
        completed = background_tasks.next_exit() => {
            Some(match completed {
                Some((name, result)) => unexpected_exit(name, result),
                None => anyhow::anyhow!("all background services exited unexpectedly"),
            })
        }
        _ = shutdown.cancelled() => {
            Some(anyhow::anyhow!("runtime requested shutdown; inspect preceding errors"))
        }
    };
    shutdown.cancel();

    if !agg_joined {
        failure = failure.or(wait_task("aggregator", agg_task).await.err());
    }
    if !api_joined {
        failure = failure.or(wait_task("api", api_task).await.err());
    }
    failure = failure.or(background_tasks.wait_all().await.err());

    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}

fn shutdown_signal() -> std::io::Result<impl std::future::Future<Output = std::io::Result<()>>> {
    // Register SIGTERM before announcing API readiness. systemd, Docker and
    // Kubernetes normally stop Unix services with SIGTERM rather than SIGINT.
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    #[cfg(unix)]
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    Ok(async move {
        #[cfg(unix)]
        {
            tokio::select! {
                _ = interrupt.recv() => Ok(()),
                _ = terminate.recv() => Ok(()),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await
        }
    })
}

#[derive(Default)]
struct TaskGroup {
    tasks: FuturesUnordered<NamedTask>,
}

type TaskCompletion = (&'static str, anyhow::Result<()>);
type NamedTask = std::pin::Pin<Box<dyn std::future::Future<Output = TaskCompletion> + Send>>;

impl TaskGroup {
    fn optional(&mut self, name: &'static str, task: Option<JoinHandle<()>>) {
        if let Some(task) = task {
            self.required(name, task);
        }
    }

    fn required(&mut self, name: &'static str, task: JoinHandle<()>) {
        // Poll the actual handles concurrently without spawning extra monitor
        // tasks. Completion order must not depend on registration order.
        self.tasks.push(Box::pin(async move {
            (name, task.await.map_err(anyhow::Error::from))
        }));
    }

    fn optional_result(
        &mut self,
        name: &'static str,
        task: Option<JoinHandle<anyhow::Result<()>>>,
    ) {
        if let Some(task) = task {
            self.tasks.push(Box::pin(async move {
                (
                    name,
                    task.await
                        .map_err(anyhow::Error::from)
                        .and_then(|result| result),
                )
            }));
        }
    }

    fn extend(&mut self, name: &'static str, tasks: impl IntoIterator<Item = JoinHandle<()>>) {
        for task in tasks {
            self.required(name, task);
        }
    }

    async fn next_exit(&mut self) -> Option<TaskCompletion> {
        self.tasks.next().await
    }

    async fn wait_all(mut self) -> anyhow::Result<()> {
        let mut failure = None;
        while let Some((name, result)) = self.next_exit().await {
            failure = failure.or(log_join_result(name, result).err());
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

async fn wait_task(name: &'static str, task: JoinHandle<()>) -> anyhow::Result<()> {
    log_join_result(name, task.await.map_err(anyhow::Error::from))
}

fn unexpected_exit(name: &'static str, result: anyhow::Result<()>) -> anyhow::Error {
    log_join_result(name, result)
        .err()
        .unwrap_or_else(|| anyhow::anyhow!("required task {name} exited unexpectedly"))
}

fn log_join_result(name: &'static str, result: anyhow::Result<()>) -> anyhow::Result<()> {
    match result {
        Ok(()) => {
            info!(task = name, "task exited");
            Ok(())
        }
        Err(error) => {
            if error
                .downcast_ref::<tokio::task::JoinError>()
                .is_some_and(|join| join.is_panic())
            {
                error!(task = name, error = %error, "task panicked");
            } else {
                error!(task = name, error = %error, "task failed");
            }
            Err(error).with_context(|| format!("{name} task failed"))
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn supervision_detects_exit_while_an_earlier_service_is_still_running() {
        let mut group = TaskGroup::default();
        let stop = CancellationToken::new();
        let task_stop = stop.clone();
        group.required(
            "running",
            tokio::spawn(async move { task_stop.cancelled().await }),
        );
        group.required("stopped-consumer", tokio::spawn(async {}));
        let (name, result) = tokio::time::timeout(Duration::from_secs(1), group.next_exit())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(name, "stopped-consumer");
        assert!(
            unexpected_exit(name, result)
                .to_string()
                .contains("stopped-consumer")
        );
        stop.cancel();
        group.wait_all().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_reports_panics_and_still_waits_for_other_services() {
        let mut group = TaskGroup::default();
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_finished = finished.clone();
        group.required(
            "crashed-consumer",
            tokio::spawn(async { panic!("fixture failure") }),
        );
        group.required(
            "draining",
            tokio::spawn(async move {
                tokio::task::yield_now().await;
                task_finished.store(true, std::sync::atomic::Ordering::SeqCst);
            }),
        );
        let error = group.wait_all().await.unwrap_err();
        assert!(error.to_string().contains("crashed-consumer"));
        assert!(finished.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn shutdown_reports_service_errors_without_a_panic() {
        let mut group = TaskGroup::default();
        group.optional_result(
            "redis_sink",
            Some(tokio::spawn(async {
                anyhow::bail!("dead-letter disk unavailable")
            })),
        );
        let error = group.wait_all().await.unwrap_err();
        assert!(format!("{error:#}").contains("dead-letter disk unavailable"));
        assert!(error.to_string().contains("redis_sink"));
    }
}
