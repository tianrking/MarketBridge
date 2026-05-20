use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::api::ApiState;
use crate::deribit_cache::DeribitOptionFilter;
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::domains::prediction::book::envelope_from_polymarket_book;

#[derive(Debug, Serialize)]
struct CatalogSource {
    source_type: &'static str,
    source: &'static str,
    venue: Option<&'static str>,
    domains: Vec<&'static str>,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct CatalogDomain {
    domain: &'static str,
    endpoint: &'static str,
    status: &'static str,
}

pub async fn sources() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "sources": [
            CatalogSource {
                source_type: "exchange",
                source: "cex_adapters",
                venue: None,
                domains: vec!["market_quote", "market_funding"],
                status: "implemented"
            },
            CatalogSource {
                source_type: "options_venue",
                source: "deribit",
                venue: Some("deribit"),
                domains: vec!["options_chain"],
                status: "implemented"
            },
            CatalogSource {
                source_type: "prediction_market",
                source: "polymarket",
                venue: Some("polymarket"),
                domains: vec!["prediction_market", "prediction_book"],
                status: "implemented"
            }
        ]
    }))
}

pub async fn domains() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "domains": [
            CatalogDomain {
                domain: "market_quote",
                endpoint: "/v1/market/quotes",
                status: "implemented"
            },
            CatalogDomain {
                domain: "options_chain",
                endpoint: "/v1/options/chains",
                status: "implemented"
            },
            CatalogDomain {
                domain: "prediction_book",
                endpoint: "/v1/prediction/books",
                status: "implemented"
            }
        ]
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
