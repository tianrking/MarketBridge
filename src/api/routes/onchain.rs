use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::onchain::OnchainTransferQuery;

#[derive(Debug, Deserialize, Default)]
pub struct OnchainTransfersHttpQuery {
    source: Option<String>,
    chain: Option<String>,
    asset: Option<String>,
    min_amount_usd: Option<f64>,
    limit: Option<usize>,
}

pub async fn v1_onchain_transfers(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<OnchainTransfersHttpQuery>,
) -> impl IntoResponse {
    let rows = state
        .onchain_store
        .query(OnchainTransferQuery {
            source: q.source.map(|x| x.trim().to_ascii_lowercase()),
            chain: q.chain.map(|x| x.trim().to_ascii_lowercase()),
            asset: q.asset.map(|x| x.trim().to_ascii_uppercase()),
            min_amount_usd: q.min_amount_usd,
            limit: q.limit.unwrap_or(500),
        })
        .await;
    Json(serde_json::json!({
        "version": "v1",
        "domain": "onchain_transfer",
        "transfers": rows
    }))
}
