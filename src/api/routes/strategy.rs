use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;

#[derive(Debug, Deserialize, Default)]
pub struct SymbolStateQuery {
    symbol: Option<String>,
    exchange: Option<String>,
}

pub async fn symbol_state(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<SymbolStateQuery>,
) -> impl IntoResponse {
    let symbol = q
        .symbol
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("BTCUSDT");
    let exchange = q
        .exchange
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Json(state.strategy_state_store.query(symbol, exchange).await)
}
