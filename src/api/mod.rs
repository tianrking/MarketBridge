use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::deribit_cache::DeribitOptionCache;
use crate::event_bus::EventBus;
use crate::metrics::AppMetrics;
use crate::polymarket_ws::PolymarketBookCache;

pub mod routes;

#[derive(Clone)]
pub struct ApiState {
    pub bus: EventBus,
    pub metrics: Arc<AppMetrics>,
    pub http: reqwest::Client,
    pub deribit_cache: DeribitOptionCache,
    pub polymarket_cache: PolymarketBookCache,
}

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/ws/ticks", get(routes::stream::ws_ticks))
        .route("/v1/market/quotes", get(routes::market::v1_market_quotes))
        .route(
            "/v1/options/chains",
            get(routes::options::v1_options_chains),
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
            "/polymarket/crypto-markets",
            get(routes::prediction::polymarket_crypto_markets),
        )
        .route("/polymarket/book", get(routes::prediction::polymarket_book))
        .route(
            "/polymarket/books",
            get(routes::prediction::polymarket_books),
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
