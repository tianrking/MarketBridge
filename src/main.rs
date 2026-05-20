mod aggregator;
mod aggregator_signal;
mod api;
mod catalog;
mod config;
mod connectors;
mod core;
mod deribit_cache;
mod domains;
mod event_bus;
mod event_snapshots;
mod klines;
mod metrics;
mod onchain;
mod order_flow;
mod polymarket_ws;
mod redis_sink;
mod router;
mod runtime;
mod source;
mod source_roadmap;
mod types;

use aggregator::SpreadAggregator;
use api::{ApiState, build_router};
use config::AppConfig;
use connectors::cex::registry::build_sources;
use deribit_cache::{
    DeribitOptionCache, spawn_binance_option_cache, spawn_bybit_option_cache,
    spawn_deribit_option_cache, spawn_okx_option_cache,
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
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use types::DataEvent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

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
    let mut tasks = handle.tasks;

    let bus = EventBus::new(cfg.runtime.broadcast_capacity, cfg.runtime.stale_ttl_ms);

    let deribit_cache = DeribitOptionCache::new(cfg.deribit.stale_ttl_ms);
    let polymarket_cache = PolymarketBookCache::new(cfg.polymarket.stale_ttl_ms);
    let kline_store = KlineStore::new(cfg.klines.sqlite_path.clone());
    let order_flow_store = OrderFlowStore::new(100_000.0);
    let onchain_store = OnchainTransferStore::default();
    let http = reqwest::Client::new();

    let api_router = build_router(ApiState {
        config: cfg.clone(),
        bus: bus.clone(),
        metrics: metrics.clone(),
        http: http.clone(),
        deribit_cache: deribit_cache.clone(),
        polymarket_cache: polymarket_cache.clone(),
        kline_store: kline_store.clone(),
        order_flow_store: order_flow_store.clone(),
        onchain_store: onchain_store.clone(),
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
            metrics.clone(),
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

    let okx_options_task = cfg.okx_options.enabled.then(|| {
        spawn_okx_option_cache(
            cfg.okx_options.clone(),
            http.clone(),
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

    let binance_options_task = cfg.binance_options.enabled.then(|| {
        spawn_binance_option_cache(
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
            polymarket_cache,
            shutdown.clone(),
        )
    });

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
    log_onchain_start(&cfg.onchain);
    let onchain_tasks = spawn_onchain_collectors(
        cfg.onchain.clone(),
        http.clone(),
        onchain_store.clone(),
        shutdown.clone(),
    );

    let (agg_tx, agg_rx) = mpsc::channel::<DataEvent>(cfg.runtime.queue_capacity);
    let router = EventRouter::new(handle.rx, agg_tx, bus.clone(), metrics.clone());
    let router_task = tokio::spawn(router.run());

    let mut agg_task = tokio::spawn(SpreadAggregator::from_config(&cfg).run(agg_rx));

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("ctrl-c received, shutting down");
            shutdown.cancel();
        }
        res = &mut agg_task => {
            match res {
                Ok(()) => info!("aggregator exited"),
                Err(e) => error!(error = %e, "aggregator task failed"),
            }
            shutdown.cancel();
        }
    }

    if let Some(redis_task) = redis_task {
        let _ = redis_task.await;
    }

    if let Some(deribit_task) = deribit_task {
        let _ = deribit_task.await;
    }

    if let Some(okx_options_task) = okx_options_task {
        let _ = okx_options_task.await;
    }

    if let Some(bybit_options_task) = bybit_options_task {
        let _ = bybit_options_task.await;
    }

    if let Some(binance_options_task) = binance_options_task {
        let _ = binance_options_task.await;
    }

    if let Some(polymarket_task) = polymarket_task {
        let _ = polymarket_task.await;
    }

    if let Some(kline_task) = kline_task {
        let _ = kline_task.await;
    }

    let _ = order_flow_task.await;

    for task in onchain_tasks {
        let _ = task.await;
    }

    for t in tasks.drain(..) {
        let _ = t.await;
    }

    let _ = router_task.await;
    let _ = api_task.await;

    Ok(())
}
