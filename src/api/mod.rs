use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get};

use crate::catalog::CatalogSource;
use crate::data_lake::DataLakeStore;
use crate::deribit_cache::DeribitOptionCache;
use crate::event_bus::EventBus;
use crate::klines::KlineStore;
use crate::metrics::AppMetrics;
use crate::onchain::OnchainTransferStore;
use crate::order_flow::OrderFlowStore;
use crate::polymarket_ws::PolymarketBookCache;

pub mod error;
pub mod routes;
pub mod streaming;
pub mod utils;

#[derive(Clone)]
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
}

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/ws/ticks", get(routes::stream::ws_ticks))
        .route("/v1/stream", get(routes::stream::v1_stream))
        .route("/v1/catalog/sources", get(routes::catalog::sources))
        .route(
            "/v1/catalog/source-roadmap",
            get(routes::catalog::source_roadmap),
        )
        .route("/v1/catalog/domains", get(routes::catalog::domains))
        .route("/v1/catalog/instruments", get(routes::catalog::instruments))
        .route("/v1/catalog/health", get(routes::catalog::health))
        .route("/v1/market/quotes", get(routes::market::v1_market_quotes))
        .route("/v1/market/basis", get(routes::market::v1_market_basis))
        .route("/v1/market/funding", get(routes::market::v1_market_funding))
        .route(
            "/v1/market/open-interest",
            get(routes::market::v1_market_open_interest),
        )
        .route("/v1/market/trades", get(routes::market::v1_market_trades))
        .route(
            "/v1/market/order-flow",
            get(routes::market::v1_market_order_flow),
        )
        .route(
            "/v1/market/order-flow/windows",
            get(routes::market::v1_market_order_flow_windows),
        )
        .route(
            "/v1/market/footprint",
            get(routes::market::v1_market_footprint),
        )
        .route("/v1/market/klines", get(routes::market::v1_market_klines))
        .route(
            "/v1/market/liquidations",
            get(routes::market::v1_market_liquidations),
        )
        .route(
            "/v1/market/order-books",
            get(routes::market::v1_market_order_books),
        )
        .route(
            "/v1/options/chains",
            get(routes::options::v1_options_chains),
        )
        .route(
            "/v1/prediction/books",
            get(routes::prediction::v1_prediction_books),
        )
        .route(
            "/v1/external/signals",
            get(routes::external::v1_external_signals),
        )
        .route(
            "/v1/onchain/transfers",
            get(routes::onchain::v1_onchain_transfers),
        )
        .route("/v1/universe/top-volume", get(routes::universe::top_volume))
        .route(
            "/v1/universe/percent-change",
            get(routes::universe::percent_change),
        )
        .route("/v1/universe/volatility", get(routes::universe::volatility))
        .route(
            "/v1/universe/spread-filter",
            get(routes::universe::spread_filter),
        )
        .route(
            "/v1/universe/cross-market",
            get(routes::universe::cross_market),
        )
        .route("/v1/universe/market-cap", get(routes::universe::market_cap))
        .route("/v1/universe/age-filter", get(routes::universe::age_filter))
        .route(
            "/v1/universe/new-listings",
            get(routes::universe::new_listings),
        )
        .route(
            "/v1/universe/delist-risk",
            get(routes::universe::delist_risk),
        )
        .route("/v1/research/features", get(routes::research::features))
        .route(
            "/v1/research/market-regime",
            get(routes::research::market_regime),
        )
        .route("/v1/storage/manifest", get(routes::storage::manifest))
        .route(
            "/v1/storage/partitions",
            delete(routes::storage::delete_partitions),
        )
        .route("/health", get(routes::system::health))
        .route("/snapshot", get(routes::legacy::snapshot))
        .route("/funding", get(routes::legacy::funding))
        .route(
            "/options/deribit/summary",
            get(routes::options::deribit_options_summary),
        )
        .route(
            "/options/deribit/live-summary",
            get(routes::options::deribit_live_options_summary),
        )
        .route(
            "/options/deribit/book",
            get(routes::options::deribit_option_book),
        )
        .route(
            "/options/bybit/book",
            get(routes::options::bybit_option_book),
        )
        .route(
            "/options/binance/book",
            get(routes::options::binance_option_book),
        )
        .route("/options/okx/book", get(routes::options::okx_option_book))
        .route(
            "/polymarket/crypto-markets",
            get(routes::prediction::polymarket_crypto_markets),
        )
        .route(
            "/polymarket/markets",
            get(routes::prediction::polymarket_markets),
        )
        .route("/polymarket/book", get(routes::prediction::polymarket_book))
        .route(
            "/polymarket/books",
            get(routes::prediction::polymarket_books),
        )
        .route(
            "/polymarket/midpoints",
            get(routes::prediction::polymarket_midpoints),
        )
        .route(
            "/polymarket/spreads",
            get(routes::prediction::polymarket_spreads),
        )
        .route(
            "/polymarket/last-trade-prices",
            get(routes::prediction::polymarket_last_trade_prices),
        )
        .route(
            "/polymarket/prices",
            get(routes::prediction::polymarket_market_prices),
        )
        .route(
            "/polymarket/prices-history",
            get(routes::prediction::polymarket_prices_history),
        )
        .route(
            "/polymarket/crypto-books",
            get(routes::prediction::polymarket_crypto_books),
        )
        .route(
            "/polymarket/live-books",
            get(routes::prediction::polymarket_live_books),
        )
        .route(
            "/polymarket/live-crypto-books",
            get(routes::prediction::polymarket_live_crypto_books),
        )
        .route("/coverage", get(routes::legacy::coverage))
        .route("/metrics", get(routes::system::metrics))
        .route("/", get(routes::system::root))
        .with_state(Arc::new(state))
}
