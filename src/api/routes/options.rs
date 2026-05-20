use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::deribit_cache::DeribitOptionFilter;
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::external::fetch_deribit_option_summaries;

#[derive(Debug, Deserialize)]
pub struct DeribitOptionsQuery {
    currency: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct DeribitLiveOptionsQuery {
    currency: Option<String>,
    option_type: Option<String>,
    strike_min: Option<f64>,
    strike_max: Option<f64>,
    expiry_after: Option<String>,
    expiry_before: Option<String>,
    include_stale: Option<bool>,
}

pub async fn deribit_options_summary(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<DeribitOptionsQuery>,
) -> impl IntoResponse {
    let currency = q.currency.unwrap_or_else(|| "BTC".to_string());
    match fetch_deribit_option_summaries(&state.http, &currency).await {
        Ok(rows) => Json(serde_json::json!({
            "source": "deribit",
            "currency": currency.to_ascii_uppercase(),
            "summaries": rows
        })),
        Err(error) => Json(serde_json::json!({
            "source": "deribit",
            "currency": currency.to_ascii_uppercase(),
            "error": error.to_string(),
            "summaries": []
        })),
    }
}

pub async fn v1_options_chains(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<DeribitLiveOptionsQuery>,
) -> impl IntoResponse {
    let rows = state
        .deribit_cache
        .filtered(DeribitOptionFilter {
            currency: q.currency.clone(),
            option_type: q.option_type,
            strike_min: q.strike_min,
            strike_max: q.strike_max,
            expiry_after: q.expiry_after,
            expiry_before: q.expiry_before,
            include_stale: q.include_stale.unwrap_or(false),
        })
        .await
        .into_iter()
        .map(envelope_from_deribit_summary)
        .collect::<Vec<_>>();

    Json(serde_json::json!({
        "version": "v1",
        "domain": "options_chain",
        "chains": rows
    }))
}

pub async fn deribit_live_options_summary(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<DeribitLiveOptionsQuery>,
) -> impl IntoResponse {
    let rows = state
        .deribit_cache
        .filtered(DeribitOptionFilter {
            currency: q.currency.clone(),
            option_type: q.option_type,
            strike_min: q.strike_min,
            strike_max: q.strike_max,
            expiry_after: q.expiry_after,
            expiry_before: q.expiry_before,
            include_stale: q.include_stale.unwrap_or(false),
        })
        .await;
    Json(serde_json::json!({
        "source": "deribit_rest_cache",
        "currency": q.currency.map(|x| x.to_ascii_uppercase()),
        "summaries": rows
    }))
}
