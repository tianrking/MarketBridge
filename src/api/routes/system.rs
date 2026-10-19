use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;

use crate::api::ApiState;
use crate::connectors::aggregate::custom_api::provider_quota_status;

pub async fn root() -> impl IntoResponse {
    Json(serde_json::json!({"service":"MarketBridge"}))
}

pub async fn workbench() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        )],
        axum::response::Html(include_str!("../../../frontend/research.html")),
    )
}
pub async fn workbench_script() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        include_str!("../../../frontend/research.js"),
    )
}
pub async fn workbench_style() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../../../frontend/research.css"),
    )
}
pub async fn workbench_example() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        include_str!("../../../examples/research/same-asset.json"),
    )
}

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"ok": true}))
}

pub async fn memory(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    Json(
        serde_json::json!({"version":"v1","domain":"retention_memory",
        "order_flow":state.order_flow_store.retention_status(true).await,
        "strategy":state.strategy_state_store.retention_status().await,
        "note":"Retained owned allocations; allocator, network and query temporaries are outside these budgets."}),
    )
}

pub async fn outbox(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    match &state.outbox {
        None => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"enabled":false})),
        ),
        Some(box_) => match box_.status().await {
            Ok(status) => (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({"enabled":true,"status":status})),
            ),
            Err(error) => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"enabled":true,"error":error.to_string()})),
            ),
        },
    }
}

pub async fn readiness(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let ingestion = state.metrics.ingestion.report();
    let mut reasons = Vec::new();
    let mut quality_reasons = Vec::new();
    if ingestion["operational_ready"] != true {
        reasons.push("configured_sources_not_fresh_or_not_processed");
    }
    if ingestion["integrity"]["has_known_loss"] == true {
        quality_reasons.push("source_admission_or_continuity_issue");
    }
    let order_quality = state.order_flow_store.subscription_quality().await;
    if order_quality.has_known_gap {
        quality_reasons.push("order_flow_subscriber_gap");
    }
    let strategy = state.strategy_state_store.retention_status().await;
    let flow = state.order_flow_store.retention_status(false).await;
    let footprint = state.order_flow_store.retention_status(true).await;
    if strategy["quality"]["has_capacity_loss"] == true {
        quality_reasons.push("strategy_retention_loss");
    }
    if state
        .strategy_state_store
        .subscription_quality()
        .await
        .has_known_gap
    {
        quality_reasons.push("strategy_subscriber_gap");
    }
    let storage = if let Some(box_) = &state.outbox {
        match box_.status().await {
            Ok(status) => {
                if status.delivery.configured && !status.delivery.verified {
                    reasons.push("durable_delivery_unverified");
                }
                serde_json::json!({"enabled":true,"status":status})
            }
            Err(_) => {
                reasons.push("outbox_unavailable");
                serde_json::json!({"enabled":true,"ready":false})
            }
        }
    } else {
        serde_json::json!({"enabled":false})
    };
    let operational_ready = reasons.is_empty();
    let data_ready =
        operational_ready && ingestion["data_ready"] == true && quality_reasons.is_empty();
    if ingestion["research_only"] != true
        && ingestion["require_complete_analytics"] == true
        && !quality_reasons.is_empty()
    {
        reasons.extend(quality_reasons.iter().copied());
    }
    let ready = reasons.is_empty();
    (
        if ready {
            axum::http::StatusCode::OK
        } else {
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        },
        Json(
            serde_json::json!({"version":"v1","domain":"runtime_readiness","ready":ready,"operational_ready":operational_ready,
            "data_ready":data_ready,"research_only":ingestion["research_only"],"reasons":reasons,"quality_reasons":quality_reasons,
            "history_quality":{"order_flow":flow["quality"],"footprint":footprint["quality"]},
            "ingestion":ingestion,"storage":storage,
            "scope":"configured source receipts/timestamps, processed stages and known quality issues; model warmup and upstream completeness are separate"}),
        ),
    )
}

pub async fn info() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "MarketBridge",
        "version": env!("CARGO_PKG_VERSION"),
        "build_revision": crate::BUILD_REVISION,
        "api_version": "v1",
        "orders_supported": false,
        "research_models": ["same-asset-spot/v1", "prefunded-taker-scenario/v1", "candidate-screen/v1", "scenario-replay/v1", "allocated-spot-portfolio/v1", "spot-derivative-basis/v1", "unit-premium/v1", "funding-rate-comparison/v1", "announcement-window/v1"],
        "research_workspace": {"version":"research-workspace/v1","storage":"SQLite immutable documents with CRC","config_hot_reload":"research scanner only; collector/server config remains startup-only","workbench":"/workbench","full_journal_replay":"CLI --replay-journal; normalized sealed files, no recorded drops"},
        "research_limits": {"body_bytes":2097152,"book_levels":200,"sizes":32,"replay_frames":512,"scan_candidates":64},
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
            "history_stablecoin_supply",
            "storage_manifest",
            "options",
            "prediction_markets",
            "external_signals",
            "stablecoin_liquidity_context",
            "defi_yield_context",
            "deribit_volatility_index_history",
            "onchain_transfers",
            "bitcoin_mempool_context",
            "bitcoin_mining_context",
            "universe",
            "research",
            "conditional_cost_curves",
            "scenario_replay",
            "paper_fill_scenarios",
            "candidate_screening",
            "integration_context",
            "websocket_stream"
        ],
        "recommended_probe_order": [
            "/v1/system/info",
            "/health",
            "/v1/catalog/sources"
        ]
    }))
}

pub async fn provider_quotas() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "domain": "provider_quota_status",
        "quotas": provider_quota_status().await,
        "notes": [
            "Rows are local shared windows for configured custom HTTP sources.",
            "An empty list means no enabled custom source initialized a named provider quota.",
            "This status is not a statement of upstream account, IP, or subscription limits."
        ]
    }))
}

pub async fn metrics(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    state.metrics.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn provider_quota_status_has_a_stable_read_only_envelope() {
        let response = provider_quotas().await.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("quota status body must be readable");
        let value: serde_json::Value =
            serde_json::from_slice(&body).expect("quota status must be JSON");
        assert_eq!(value["version"], "v1");
        assert_eq!(value["domain"], "provider_quota_status");
        assert!(value["quotas"].is_array());
        assert!(value["notes"].is_array());
    }
}
