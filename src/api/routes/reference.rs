use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use crate::api::ApiState;
use crate::venue_status::VenueAssetStatusObservation;

#[derive(Debug, Deserialize, Default)]
pub struct SupplyQuery {
    asset_id: Option<String>,
    perp_symbol: Option<String>,
}

pub async fn supply(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<SupplyQuery>,
) -> Json<serde_json::Value> {
    let rows = if let Some(symbol) = query.perp_symbol.as_deref() {
        state
            .supply_store
            .for_perp_symbol(symbol)
            .await
            .into_iter()
            .collect()
    } else {
        state.supply_store.all().await
    };
    let wanted = query.asset_id.as_deref();
    let rows = rows
        .into_iter()
        .filter(|row| wanted.is_none_or(|asset_id| row.asset_id == asset_id))
        .collect::<Vec<_>>();
    Json(json!({
        "version":"v1",
        "domain":"supply_snapshot_reference",
        "rows": rows,
        "limitations":[
            "provider-reported circulating market cap is reference data, not a free-float or settlement guarantee",
            "rows exist only for explicit configured identity mappings; tickers are never auto-mapped"
        ]
    }))
}

#[derive(Debug, Deserialize, Default)]
pub struct VenueStatusQuery {
    venue: Option<String>,
    asset_id: Option<String>,
}

pub async fn venue_asset_status(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<VenueStatusQuery>,
) -> Json<serde_json::Value> {
    let rows = state
        .venue_status_store
        .query(query.venue.as_deref(), query.asset_id.as_deref())
        .await;
    Json(json!({
        "version":"v1",
        "domain":"venue_asset_status",
        "rows": rows,
        "limitations":[
            "account_observed rows are account-local evidence and do not prove a global venue status",
            "network-specific status must not be generalized to every chain or asset operation"
        ]
    }))
}

pub async fn ingest_venue_asset_status(
    State(state): State<Arc<ApiState>>,
    Json(observation): Json<VenueAssetStatusObservation>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state
        .venue_status_store
        .insert(observation.clone())
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":error.to_string()})),
            )
        })?;
    Ok(Json(json!({
        "accepted": true,
        "observation": observation,
        "execution_boundary":"read_only_reference_evidence_no_exchange_credentials_or_orders"
    })))
}
