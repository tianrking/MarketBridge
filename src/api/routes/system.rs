use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;

use crate::api::ApiState;

pub async fn root() -> impl IntoResponse {
    Json(serde_json::json!({"service":"MarketBridge"}))
}

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"ok": true}))
}

pub async fn metrics(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    state.metrics.render()
}
