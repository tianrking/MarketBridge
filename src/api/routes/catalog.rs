use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::api::ApiState;
use crate::catalog::{domain_catalog, health_status, source_catalog};
use crate::deribit_cache::DeribitOptionFilter;
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::domains::prediction::book::envelope_from_polymarket_book;

#[derive(Debug, Serialize)]
struct CatalogHealth {
    source: String,
    domain: &'static str,
    records: usize,
    stale_records: usize,
    last_received_at_ms: Option<u64>,
    status: &'static str,
}

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

pub async fn health(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mut rows = Vec::new();

    let mut quote_health = HashMap::<String, (usize, usize, Option<u64>)>::new();
    for quote in state.bus.quote_snapshot_all().await {
        let entry = quote_health
            .entry(quote.source_ref.source.clone())
            .or_insert((0, 0, None));
        entry.0 += 1;
        if quote.freshness.stale {
            entry.1 += 1;
        }
        entry.2 = entry.2.max(Some(quote.freshness.ts_received));
    }
    for (source, (records, stale_records, last_received_at_ms)) in quote_health {
        rows.push(CatalogHealth {
            source,
            domain: "market_quote",
            records,
            stale_records,
            last_received_at_ms,
            status: health_status(records, stale_records),
        });
    }

    let option_rows = state
        .deribit_cache
        .filtered(DeribitOptionFilter {
            include_stale: true,
            ..Default::default()
        })
        .await;
    let mut option_health = HashMap::<String, (usize, usize, Option<u64>)>::new();
    for option in option_rows {
        let entry = option_health
            .entry(option.summary.venue.clone())
            .or_insert((0, 0, None));
        entry.0 += 1;
        if option.stale {
            entry.1 += 1;
        }
        entry.2 = entry.2.max(Some(option.received_at_ms));
    }
    for (source, (records, stale_records, last_received_at_ms)) in option_health {
        rows.push(CatalogHealth {
            source,
            domain: "options_chain",
            records,
            stale_records,
            last_received_at_ms,
            status: health_status(records, stale_records),
        });
    }

    let polymarket_rows = state.polymarket_cache.all().await;
    let polymarket_records = polymarket_rows.len();
    let polymarket_stale = polymarket_rows.iter().filter(|row| row.stale).count();
    let polymarket_last = polymarket_rows.iter().map(|row| row.received_at_ms).max();
    rows.push(CatalogHealth {
        source: "polymarket".to_string(),
        domain: "prediction_book",
        records: polymarket_records,
        stale_records: polymarket_stale,
        last_received_at_ms: polymarket_last,
        status: health_status(polymarket_records, polymarket_stale),
    });

    rows.sort_by(|a, b| a.source.cmp(&b.source).then(a.domain.cmp(b.domain)));
    Json(serde_json::json!({
        "version": "v1",
        "health": rows
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
