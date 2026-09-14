use std::sync::Arc;

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, post};

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
use crate::supply::{SupplyReferenceService, SupplySnapshotStore};
use crate::venue_status::VenueAssetStatusStore;

pub mod cors;
pub mod error;
pub mod guard;
pub mod routes;
pub mod snapshot_stream;
pub mod streaming;
pub mod utils;

macro_rules! get_routes {
    ($router:expr, $( $path:literal => $handler:path ),+ $(,)?) => {
        $router$(.route($path, get($handler).options(cors::preflight)))+
    };
}

pub struct ApiState {
    pub research_control: crate::research_control::ResearchControl,
    pub research_store: crate::research_store::ResearchStore,
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
    pub supply_store: SupplySnapshotStore,
    pub supply_service: SupplyReferenceService,
    pub venue_status_store: VenueAssetStatusStore,
    pub snapshot_stream_hub: snapshot_stream::SnapshotStreamHub,
    pub api_access_guard: guard::ApiAccessGuard,
    pub api_cors: cors::ApiCors,
}

impl ApiState {
    pub fn api_access_guard_from_runtime(runtime: &RuntimeConfig) -> guard::ApiAccessGuard {
        guard::ApiAccessGuard::from_runtime(runtime)
    }

    pub fn api_cors_from_runtime(runtime: &RuntimeConfig) -> cors::ApiCors {
        cors::ApiCors::from_runtime(runtime)
    }
}

pub fn build_router(state: ApiState) -> Router {
    let api_access_guard = state.api_access_guard.clone();
    let api_cors = state.api_cors.clone();
    let router = get_routes!(
        Router::new(),
        "/ws/ticks" => routes::stream::ws_ticks,
        "/v1/stream" => routes::stream::v1_stream,
        "/v1/catalog/sources" => routes::catalog::sources,
        "/v1/catalog/search" => routes::catalog::search,
        "/v1/catalog/markets" => routes::catalog::markets,
        "/v1/catalog/perpetuals" => routes::catalog::perpetuals,
        "/v1/catalog/source-roadmap" => routes::catalog::source_roadmap,
        "/v1/catalog/domains" => routes::catalog::domains,
        "/v1/catalog/instruments" => routes::catalog::instruments,
        "/v1/catalog/health" => routes::catalog::health,
        "/v1/market/quotes" => routes::market::v1_market_quotes,
        "/v1/market/basis" => routes::market::v1_market_basis,
        "/v1/market/funding" => routes::market::v1_market_funding,
        "/v1/market/perpetual-funding" => routes::market::v1_market_perpetual_funding,
        "/v1/market/predicted-funding" => routes::market::v1_market_predicted_funding,
        "/v1/market/open-interest" => routes::market::v1_market_open_interest,
        "/v1/market/adl-risk" => routes::market::v1_market_adl_risk,
        "/v1/market/trades" => routes::market::v1_market_trades,
        "/v1/market/order-flow" => routes::market::v1_market_order_flow,
        "/v1/market/order-flow/windows" => routes::market::v1_market_order_flow_windows,
        "/v1/market/footprint" => routes::market::v1_market_footprint,
        "/v1/market/klines" => routes::market::v1_market_klines,
        "/v1/history/candles" => routes::history::candles,
        "/v1/history/liquidations" => routes::history::liquidations,
        "/v1/history/open-interest" => routes::history::open_interest,
        "/v1/history/taker-volume" => routes::history::taker_volume,
        "/v1/history/account-ratio" => routes::history::account_ratio,
        "/v1/history/historical-volatility" => routes::history::historical_volatility,
        "/v1/history/volatility-index" => routes::history::volatility_index,
        "/v1/history/basis" => routes::history::basis,
        "/v1/history/trades" => routes::history::trades,
        "/v1/market/liquidations" => routes::market::v1_market_liquidations,
        "/v1/market/order-books" => routes::market::v1_market_order_books,
        "/v1/options/chains" => routes::options::v1_options_chains,
        "/v1/prediction/books" => routes::prediction::v1_prediction_books,
        "/v1/prediction/trades" => routes::prediction::v1_prediction_trades,
        "/v1/external/signals" => routes::external::v1_external_signals,
        "/v1/external/global-market" => routes::external::v1_external_global_market,
        "/v1/external/stablecoins" => routes::external::v1_external_stablecoins,
        "/v1/external/defi-yields" => routes::external::v1_external_defi_yields,
        "/v1/external/weather" => routes::external::v1_external_weather,
        "/v1/onchain/transfers" => routes::onchain::v1_onchain_transfers,
        "/v1/onchain/mempool" => routes::onchain::v1_onchain_mempool,
        "/v1/onchain/mining" => routes::onchain::v1_onchain_mining,
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
        "/v1/research/squeeze/scan" => routes::strategy::squeeze_scan,
        "/v1/reference/supply" => routes::reference::supply,
        "/v1/reference/venue-asset-status" => routes::reference::venue_asset_status,
        "/v1/storage/manifest" => routes::storage::manifest,
        "/v1/integration/context" => routes::integration::context,
        "/v1/integration/capabilities" => routes::integration::capabilities,
        "/v1/system/info" => routes::system::info,
        "/v1/system/provider-quotas" => routes::system::provider_quotas,
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
            "/v1/research/control",
            get(routes::opportunities::control_status)
                .post(routes::opportunities::control_apply)
                .options(cors::preflight),
        )
        .route(
            "/v1/research/workspace",
            post(routes::opportunities::workspace).options(cors::preflight),
        )
        .route(
            "/v1/research/scan",
            post(routes::opportunities::scan_batch).options(cors::preflight),
        )
        .route(
            "/v1/research/scan-live",
            post(routes::opportunities::scan_live_batch).options(cors::preflight),
        )
        .route(
            "/v1/research/paper",
            post(routes::opportunities::paper).options(cors::preflight),
        )
        .route(
            "/v1/research/evaluate-live",
            post(routes::opportunities::evaluate_live).options(cors::preflight),
        )
        .route(
            "/v1/research/evaluate",
            post(routes::opportunities::evaluate).options(cors::preflight),
        )
        .route(
            "/v1/research/replay",
            post(routes::opportunities::replay).options(cors::preflight),
        )
        .route(
            "/v1/research/squeeze/archive",
            post(routes::strategy::archive_squeeze_scan).options(cors::preflight),
        )
        .route(
            "/v1/reference/venue-asset-status",
            post(routes::reference::ingest_venue_asset_status),
        )
        .route(
            "/v1/storage/partitions",
            delete(routes::storage::delete_partitions).options(cors::preflight),
        )
        .layer(middleware::from_fn_with_state(
            api_access_guard,
            guard::api_guard,
        ))
        .layer(middleware::from_fn_with_state(
            api_cors,
            cors::cors_middleware,
        ))
        .with_state(Arc::new(state))
        // Public immutable assets only; all data/mutations remain behind API guard.
        .route("/workbench", get(routes::system::workbench))
        .route("/workbench/app.js", get(routes::system::workbench_script))
        .route(
            "/workbench/styles.css",
            get(routes::system::workbench_style),
        )
        .route(
            "/workbench/example.json",
            get(routes::system::workbench_example),
        )
}
