use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};

#[derive(Debug, Deserialize, Default)]
pub struct ExternalSignalsQuery {
    sources: Option<String>,
    categories: Option<String>,
    symbols: Option<String>,
    metrics: Option<String>,
}

pub async fn v1_external_signals(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ExternalSignalsQuery>,
) -> impl IntoResponse {
    let sources = q.sources.map(parse_csv_set_lower);
    let categories = q.categories.map(parse_csv_set_lower);
    let symbols = q.symbols.map(parse_csv_set_upper);
    let metrics = q.metrics.map(parse_csv_set_lower);

    let mut rows = state
        .bus
        .external_signal_snapshot_all()
        .await
        .into_iter()
        .filter(|row| {
            sources
                .as_ref()
                .is_none_or(|set| set.contains(&row.source.to_ascii_lowercase()))
        })
        .filter(|row| {
            categories
                .as_ref()
                .is_none_or(|set| set.contains(&row.category.to_ascii_lowercase()))
        })
        .filter(|row| {
            symbols.as_ref().is_none_or(|set| {
                row.symbol
                    .as_deref()
                    .is_none_or(|symbol| set.contains(&symbol.to_ascii_uppercase()))
            })
        })
        .filter(|row| {
            metrics
                .as_ref()
                .is_none_or(|set| set.contains(&row.metric.to_ascii_lowercase()))
        })
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        a.source
            .cmp(b.source)
            .then(a.category.cmp(&b.category))
            .then(a.metric.cmp(&b.metric))
    });

    Json(serde_json::json!({
        "version": "v1",
        "domain": "external_signal",
        "signals": rows
    }))
}
