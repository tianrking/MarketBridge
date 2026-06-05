mod aggregator;
mod aggregator_signal;
mod api;
mod catalog;
mod clickhouse_sink;
mod config;
mod connectors;
mod core;
mod data_lake;
mod deribit_cache;
mod domains;
mod event_bus;
mod event_snapshots;
mod klines;
mod load_test;
mod market_discovery;
mod metrics;
mod onchain;
mod order_flow;
mod polymarket_ws;
mod redis_sink;
mod router;
mod runtime;
mod source;
mod source_roadmap;
mod strategy_state;
mod types;

use aggregator::SpreadAggregator;
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
use klines::{KlineStore, spawn_kline_service};
use metrics::AppMetrics;
use onchain::{OnchainTransferStore, log_onchain_start, spawn_onchain_collectors};
use order_flow::{OrderFlowStore, spawn_order_flow_service};
use polymarket_ws::{PolymarketBookCache, spawn_polymarket_ws_cache};
use redis_sink::spawn_redis_sink;
use router::EventRouter;
use runtime::SourceRuntime;
use strategy_state::{StrategyStateStore, spawn_strategy_state_service};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use types::DataEvent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let args: Vec<String> = std::env::args().collect();
    if let Some(load_test_cfg) = load_test::load_test_config_from_args(&args) {
        load_test::run_load_test(load_test_cfg).await;
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cfg = AppConfig::load()?;
    let metrics = AppMetrics::new();
    let runtime = SourceRuntime::new(
        cfg.runtime.queue_capacity,
        cfg.backpressure_mode(),
        metrics.clone(),
    );
    let sources = build_sources(&cfg);

    let handle = runtime.spawn_sources(sources);
    let shutdown = handle.shutdown.clone();
    let tasks = handle.tasks;

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

    let order_flow_store = OrderFlowStore::new(cfg.runtime.order_flow_large_trade_notional_usdt);
    let onchain_store = OnchainTransferStore::default();
    let strategy_state_store = StrategyStateStore::new();
    let source_catalog = catalog::source_catalog_for_config(&cfg);
    let http = reqwest::Client::new();

    let api_router = build_router(ApiState {
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
        snapshot_stream_hub: snapshot_stream_hub.clone(),
        api_access_guard: ApiState::api_access_guard_from_runtime(&cfg.runtime),
    });
    let api_addr = cfg.runtime.api_addr.clone();
    let api_shutdown = shutdown.clone();
    let api_task = tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(&api_addr).await {
            Ok(l) => l,
            Err(e) => {
                error!(addr = %api_addr, error = %e, "api bind failed");
                return;
            }
        };
        info!(addr=%api_addr, "api server started");
        if let Err(e) = axum::serve(listener, api_router)
            .with_graceful_shutdown(async move {
                api_shutdown.cancelled().await;
            })
            .await
        {
            error!(error=%e, "api server failed");
        }
    });

    let redis_task = cfg.runtime.redis_url.clone().map(|url| {
        spawn_redis_sink(
            bus.clone(),
            url,
            cfg.runtime.redis_stream_prefix.clone(),
            cfg.runtime.redis_dead_letter_path.clone(),
            metrics.clone(),
            shutdown.clone(),
        )
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
    );
    let router_task = tokio::spawn(router.run());

    let mut agg_task = tokio::spawn(SpreadAggregator::from_config(&cfg).run(agg_rx));

    let mut background_tasks = TaskGroup::default();
    background_tasks.optional("redis_sink", redis_task);
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
    background_tasks.required("router", router_task);
    background_tasks.required("api", api_task);

    let mut agg_joined = false;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("ctrl-c received, shutting down");
            shutdown.cancel();
        }
        res = &mut agg_task => {
            log_join_result("aggregator", res);
            agg_joined = true;
            shutdown.cancel();
        }
    }

    if !agg_joined {
        wait_task("aggregator", agg_task).await;
    }
    background_tasks.wait_all().await;

    Ok(())
}

#[derive(Default)]
struct TaskGroup {
    tasks: Vec<NamedTask>,
}

struct NamedTask {
    name: &'static str,
    task: JoinHandle<()>,
}

impl TaskGroup {
    fn optional(&mut self, name: &'static str, task: Option<JoinHandle<()>>) {
        if let Some(task) = task {
            self.required(name, task);
        }
    }

    fn required(&mut self, name: &'static str, task: JoinHandle<()>) {
        self.tasks.push(NamedTask { name, task });
    }

    fn extend(&mut self, name: &'static str, tasks: impl IntoIterator<Item = JoinHandle<()>>) {
        self.tasks
            .extend(tasks.into_iter().map(|task| NamedTask { name, task }));
    }

    async fn wait_all(self) {
        for task in self.tasks {
            wait_task(task.name, task.task).await;
        }
    }
}

async fn wait_task(name: &'static str, task: JoinHandle<()>) {
    log_join_result(name, task.await);
}

fn log_join_result(name: &'static str, result: Result<(), tokio::task::JoinError>) {
    match result {
        Ok(()) => info!(task = name, "task exited"),
        Err(error) if error.is_panic() => error!(task = name, error = %error, "task panicked"),
        Err(error) => error!(task = name, error = %error, "task failed"),
    }
}
