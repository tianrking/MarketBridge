use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;

use crate::api::ApiState;
use crate::types::now_ms;

static SQUEEZE_ARCHIVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Default)]
pub struct SymbolStateQuery {
    symbol: Option<String>,
    exchange: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SqueezeScanQuery {
    exchange: Option<String>,
    #[serde(default = "default_max_data_age_ms")]
    max_data_age_ms: u64,
    #[serde(default)]
    minimum_score: i32,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_max_data_age_ms() -> u64 {
    15_000
}

fn default_limit() -> usize {
    100
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

/// Current read-only squeeze research ranking. It consumes only symbols that
/// this running process has already observed; it never expands subscriptions
/// or calls an execution endpoint.
pub async fn squeeze_scan(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<SqueezeScanQuery>,
) -> Result<Json<crate::squeeze_radar::SqueezeScanResponse>, (StatusCode, Json<serde_json::Value>)>
{
    let max_data_age_ms = query.max_data_age_ms.clamp(1_000, 60_000);
    let rows = state.strategy_state_store.query_all().await;
    let exchange = query
        .exchange
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rows = rows
        .into_iter()
        .filter(|row| exchange.is_none_or(|wanted| row.exchange.eq_ignore_ascii_case(wanted)))
        .collect();
    Ok(Json(crate::squeeze_radar::scan(
        rows,
        now_ms(),
        max_data_age_ms,
        query.minimum_score.clamp(0, 10),
        query.limit.clamp(1, 500),
    )))
}

/// Stores an immutable evidence snapshot for a later experiment or review.
/// The response is still only research data; no order or account state exists
/// in this application.
pub async fn archive_squeeze_scan(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<SqueezeScanQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let max_data_age_ms = query.max_data_age_ms.clamp(1_000, 60_000);
    let rows = state.strategy_state_store.query_all().await;
    let exchange = query
        .exchange
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let report = crate::squeeze_radar::scan(
        rows.into_iter()
            .filter(|row| exchange.is_none_or(|wanted| row.exchange.eq_ignore_ascii_case(wanted)))
            .collect(),
        now_ms(),
        max_data_age_ms,
        query.minimum_score.clamp(0, 10),
        query.limit.clamp(1, 500),
    );
    let id = format!(
        "squeeze-scan-{}-{}",
        report.generated_at_ms,
        SQUEEZE_ARCHIVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let payload = serde_json::to_value(&report).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("cannot serialize squeeze scan: {error}")})),
        )
    })?;
    let document = state
        .research_store
        .insert("squeeze-scans", &id, report.generated_at_ms, &payload)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("cannot archive squeeze scan: {error}")})),
            )
        })?;
    Ok(Json(json!({
        "document": document,
        "model_version": crate::squeeze_radar::MODEL_VERSION,
        "execution_boundary": "read_only_research_no_orders_or_wallet_signing"
    })))
}
