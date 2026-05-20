use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;

use crate::api::ApiState;
use crate::catalog::{domain_catalog, source_catalog};
use crate::deribit_cache::DeribitOptionFilter;
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::domains::prediction::book::envelope_from_polymarket_book;

pub async fn sources() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "sources": source_catalog()
    }))
}

pub async fn domains() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "domains": domain_catalog()
    }))
}

pub async fn instruments(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mut seen = HashSet::new();
    let mut instruments = Vec::new();

    for quote in state.bus.quote_snapshot_all().await {
        if seen.insert(quote.instrument_ref.instrument_id.clone()) {
            instruments.push(quote.instrument_ref);
        }
    }

    for option in state
        .deribit_cache
        .filtered(DeribitOptionFilter {
            include_stale: true,
            ..Default::default()
        })
        .await
        .into_iter()
        .map(envelope_from_deribit_summary)
    {
        if seen.insert(option.instrument_ref.instrument_id.clone()) {
            instruments.push(option.instrument_ref);
        }
    }

    for book in state
        .polymarket_cache
        .all()
        .await
        .into_iter()
        .map(envelope_from_polymarket_book)
    {
        if seen.insert(book.instrument_ref.instrument_id.clone()) {
            instruments.push(book.instrument_ref);
        }
    }

    instruments.sort_by(|a, b| a.instrument_id.cmp(&b.instrument_id));
    Json(serde_json::json!({
        "version": "v1",
        "instruments": instruments
    }))
}
