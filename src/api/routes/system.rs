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

pub async fn info() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "MarketBridge",
        "version": env!("CARGO_PKG_VERSION"),
        "api_version": "v1",
        "status": "ok",
        "local_ui": {
            "browser_localhost_supported": true,
            "cors_supported": true,
            "private_network_access_supported": true,
            "default_base_urls": [
                "http://127.0.0.1:8080",
                "http://localhost:8080"
            ]
        },
        "auth": {
            "api_key_headers": ["x-api-key", "authorization: Bearer <key>"]
        },
        "capabilities": [
            "health",
            "catalog",
            "market_discovery",
            "perpetual_funding",
            "market_snapshots",
            "basis",
            "order_flow",
            "history_candles",
            "storage_manifest",
            "options",
            "prediction_markets",
            "external_signals",
            "onchain_transfers",
            "universe",
            "research",
            "agent_context",
            "websocket_stream"
        ],
        "recommended_probe_order": [
            "/v1/system/info",
            "/health",
            "/v1/catalog/sources"
        ]
    }))
}

pub async fn metrics(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    state.metrics.render()
}
