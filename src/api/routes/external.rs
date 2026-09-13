use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::api::error::upstream_error;
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};
use crate::connectors::weather::{OpenMeteoRequest, fetch_open_meteo};

#[derive(Debug, Deserialize, Default)]
pub struct ExternalSignalsQuery {
    sources: Option<String>,
    categories: Option<String>,
    symbols: Option<String>,
    metrics: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct WeatherQuery {
    latitude: f64,
    longitude: f64,
    mode: Option<String>,
    timezone: Option<String>,
    forecast_days: Option<u8>,
    past_days: Option<u8>,
    start_date: Option<String>,
    end_date: Option<String>,
    hourly: Option<String>,
    daily: Option<String>,
}

pub async fn v1_external_weather(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<WeatherQuery>,
) -> impl IntoResponse {
    let request = OpenMeteoRequest {
        latitude: q.latitude,
        longitude: q.longitude,
        mode: q.mode.unwrap_or_else(|| "forecast".to_string()),
        timezone: q.timezone.unwrap_or_else(|| "UTC".to_string()),
        forecast_days: q.forecast_days,
        past_days: q.past_days,
        start_date: q.start_date,
        end_date: q.end_date,
        hourly: q.hourly,
        daily: q.daily,
    };
    match fetch_open_meteo(&state.http, &request).await {
        Ok(data) => Json(serde_json::json!({
            "version": "v1",
            "domain": "weather_observation",
            "source": "open_meteo",
            "request": {
                "latitude": request.latitude,
                "longitude": request.longitude,
                "mode": request.mode,
                "timezone": request.timezone,
                "forecast_days": request.forecast_days,
                "past_days": request.past_days,
                "start_date": request.start_date,
                "end_date": request.end_date,
                "hourly": request.hourly,
                "daily": request.daily
            },
            "data": data,
            "limitations": [
                "weather grids and model revisions are provider observations, not settlement truth",
                "location-to-market identity and event resolution rules must be supplied by the caller",
                "this read-only endpoint does not produce probabilities or trade instructions"
            ]
        }))
        .into_response(),
        Err(error) if error.to_string().contains("outside") || error.to_string().contains("requires") || error.to_string().contains("must") || error.to_string().contains("unsupported") => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"source":"open_meteo", "error": error.to_string()})),
        )
            .into_response(),
        Err(error) => upstream_error("open_meteo", error),
    }
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
