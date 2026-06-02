use std::sync::Arc;

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get};

use crate::catalog::CatalogSource;
use crate::config::RuntimeConfig;
use crate::data_lake::DataLakeStore;
use crate::deribit_cache::DeribitOptionCache;
use crate::event_bus::EventBus;
use crate::klines::KlineStore;
use crate::metrics::AppMetrics;
use crate::onchain::OnchainTransferStore;
use crate::order_flow::OrderFlowStore;
use crate::polymarket_ws::PolymarketBookCache;
use crate::strategy_state::StrategyStateStore;

pub mod error;
pub mod guard;
pub mod routes;
pub mod snapshot_stream;
pub mod streaming;
pub mod utils;

macro_rules! get_routes {
    ($router:expr, $( $path:literal => $handler:path ),+ $(,)?) => {
        $router$(.route($path, get($handler)))+
    };
}

pub struct ApiState {
    pub source_catalog: Vec<CatalogSource>,
    pub bus: EventBus,
    pub metrics: Arc<AppMetrics>,
    pub http: reqwest::Client,
    pub deribit_cache: DeribitOptionCache,
    pub polymarket_cache: PolymarketBookCache,
    pub kline_store: KlineStore,
    pub data_lake_store: DataLakeStore,
    pub order_flow_store: OrderFlowStore,
    pub onchain_store: OnchainTransferStore,
    pub strategy_state_store: StrategyStateStore,
    pub snapshot_stream_hub: snapshot_stream::SnapshotStreamHub,
    pub api_access_guard: guard::ApiAccessGuard,
}

impl ApiState {
    pub fn api_access_guard_from_runtime(runtime: &RuntimeConfig) -> guard::ApiAccessGuard {
        guard::ApiAccessGuard::from_runtime(runtime)
    }
}

pub fn build_router(state: ApiState) -> Router {
    let api_access_guard = state.api_access_guard.clone();
    let router = get_routes!(
        Router::new(),
        "/ws/ticks" => routes::stream::ws_ticks,
        "/v1/stream" => routes::stream::v1_stream,
        "/v1/catalog/sources" => routes::catalog::sources,
        "/v1/catalog/source-roadmap" => routes::catalog::source_roadmap,
        "/v1/catalog/domains" => routes::catalog::domains,
        "/v1/catalog/instruments" => routes::catalog::instruments,
        "/v1/catalog/health" => routes::catalog::health,
        "/v1/market/quotes" => routes::market::v1_market_quotes,
        "/v1/market/basis" => routes::market::v1_market_basis,
        "/v1/market/funding" => routes::market::v1_market_funding,
        "/v1/market/open-interest" => routes::market::v1_market_open_interest,
        "/v1/market/trades" => routes::market::v1_market_trades,
        "/v1/market/order-flow" => routes::market::v1_market_order_flow,
        "/v1/market/order-flow/windows" => routes::market::v1_market_order_flow_windows,
        "/v1/market/footprint" => routes::market::v1_market_footprint,
        "/v1/market/klines" => routes::market::v1_market_klines,
        "/v1/history/candles" => routes::history::candles,
        "/v1/market/liquidations" => routes::market::v1_market_liquidations,
        "/v1/market/order-books" => routes::market::v1_market_order_books,
        "/v1/options/chains" => routes::options::v1_options_chains,
        "/v1/prediction/books" => routes::prediction::v1_prediction_books,
        "/v1/external/signals" => routes::external::v1_external_signals,
        "/v1/onchain/transfers" => routes::onchain::v1_onchain_transfers,
        "/v1/universe/top-volume" => routes::universe::top_volume,
        "/v1/universe/percent-change" => routes::universe::percent_change,
        "/v1/universe/volatility" => routes::universe::volatility,
        "/v1/universe/spread-filter" => routes::universe::spread_filter,
        "/v1/universe/cross-market" => routes::universe::cross_market,
        "/v1/universe/market-cap" => routes::universe::market_cap,
        "/v1/universe/age-filter" => routes::universe::age_filter,
        "/v1/universe/new-listings" => routes::universe::new_listings,
        "/v1/universe/delist-risk" => routes::universe::delist_risk,
        "/v1/research/features" => routes::research::features,
        "/v1/research/market-regime" => routes::research::market_regime,
        "/v1/research/symbol-state" => routes::strategy::symbol_state,
        "/v1/storage/manifest" => routes::storage::manifest,
        "/v1/agent/context" => routes::agent::context,
        "/v1/agent/capabilities" => routes::agent::capabilities,
        "/health" => routes::system::health,
        "/snapshot" => routes::legacy::snapshot,
        "/funding" => routes::legacy::funding,
        "/options/deribit/summary" => routes::options::deribit_options_summary,
        "/options/deribit/live-summary" => routes::options::deribit_live_options_summary,
        "/options/deribit/book" => routes::options::deribit_option_book,
        "/options/bybit/book" => routes::options::bybit_option_book,
        "/options/binance/book" => routes::options::binance_option_book,
        "/options/okx/book" => routes::options::okx_option_book,
        "/polymarket/crypto-markets" => routes::prediction::polymarket_crypto_markets,
        "/polymarket/markets" => routes::prediction::polymarket_markets,
        "/polymarket/book" => routes::prediction::polymarket_book,
        "/polymarket/books" => routes::prediction::polymarket_books,
        "/polymarket/midpoints" => routes::prediction::polymarket_midpoints,
        "/polymarket/spreads" => routes::prediction::polymarket_spreads,
        "/polymarket/last-trade-prices" => routes::prediction::polymarket_last_trade_prices,
        "/polymarket/prices" => routes::prediction::polymarket_market_prices,
        "/polymarket/prices-history" => routes::prediction::polymarket_prices_history,
        "/polymarket/crypto-books" => routes::prediction::polymarket_crypto_books,
        "/polymarket/live-books" => routes::prediction::polymarket_live_books,
        "/polymarket/live-crypto-books" => routes::prediction::polymarket_live_crypto_books,
        "/coverage" => routes::legacy::coverage,
        "/metrics" => routes::system::metrics,
        "/" => routes::system::root,
    );
    router
        .route(
            "/v1/storage/partitions",
            delete(routes::storage::delete_partitions),
        )
        .layer(middleware::from_fn_with_state(
            api_access_guard,
            guard::api_guard,
        ))
        .with_state(Arc::new(state))
}
