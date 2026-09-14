use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::Value;

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

pub async fn v1_external_global_market(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let source = "coingecko_global";
    let response = state
        .http
        .get("https://api.coingecko.com/api/v3/global")
        .send()
        .await
        .and_then(|response| response.error_for_status());
    match response {
        Ok(response) => match response.json::<Value>().await {
            Ok(payload) => match normalize_coingecko_global(&payload) {
                Some(data) => Json(serde_json::json!({
                    "version": "v1",
                    "domain": "global_market_context",
                    "source": source,
                    "coverage": "provider_snapshot",
                    "data": data,
                    "limitations": [
                        "global market cap and dominance are provider aggregates, not exchange-executable prices",
                        "the snapshot is not a historical dominance series and does not establish causality",
                        "no order, wallet, signing or execution path is included"
                    ]
                }))
                .into_response(),
                None => (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({
                        "version": "v1",
                        "domain": "global_market_context",
                        "source": source,
                        "error": "CoinGecko global payload missing required data"
                    })),
                )
                    .into_response(),
            },
            Err(error) => upstream_error(source, error),
        },
        Err(error) => upstream_error(source, error),
    }
}

fn normalize_coingecko_global(payload: &Value) -> Option<Value> {
    let data = payload.get("data")?;
    let total_market_cap_usd = data
        .pointer("/total_market_cap/usd")
        .and_then(Value::as_f64)?;
    let total_volume_usd = data.pointer("/total_volume/usd").and_then(Value::as_f64);
    let market_cap_percentage = data.get("market_cap_percentage")?.clone();
    let btc_dominance_pct = market_cap_percentage.get("btc").and_then(Value::as_f64);
    let eth_dominance_pct = market_cap_percentage.get("eth").and_then(Value::as_f64);
    let active_cryptocurrencies = data.get("active_cryptocurrencies").and_then(Value::as_u64);
    let markets = data.get("markets").and_then(Value::as_u64);
    let updated_at_ms = data
        .get("updated_at")
        .and_then(Value::as_u64)
        .map(|seconds| seconds.saturating_mul(1_000));
    Some(serde_json::json!({
        "total_market_cap_usd": total_market_cap_usd,
        "total_volume_usd": total_volume_usd,
        "market_cap_change_24h_pct": data.get("market_cap_change_percentage_24h_usd").and_then(Value::as_f64),
        "volume_change_24h_pct": data.get("volume_change_percentage_24h_usd").and_then(Value::as_f64),
        "btc_dominance_pct": btc_dominance_pct,
        "eth_dominance_pct": eth_dominance_pct,
        "market_cap_percentage": market_cap_percentage,
        "active_cryptocurrencies": active_cryptocurrencies,
        "markets": markets,
        "updated_at_ms": updated_at_ms,
        "source": "coingecko_global"
    }))
}

#[cfg(test)]
mod tests {
    use super::normalize_coingecko_global;

    #[test]
    fn normalizes_global_market_dominance_and_totals() {
        let payload = serde_json::json!({
            "data": {
                "total_market_cap": {"usd": 2_000_000_000_000.0},
                "total_volume": {"usd": 80_000_000_000.0},
                "market_cap_percentage": {"btc": 52.5, "eth": 17.2},
                "market_cap_change_percentage_24h_usd": -1.5,
                "active_cryptocurrencies": 12000,
                "markets": 900,
                "updated_at": 1700000000
            }
        });
        let data = normalize_coingecko_global(&payload).expect("normalized global context");
        assert_eq!(data["btc_dominance_pct"], serde_json::json!(52.5));
        assert_eq!(
            data["total_market_cap_usd"],
            serde_json::json!(2_000_000_000_000.0)
        );
        assert_eq!(
            data["updated_at_ms"],
            serde_json::json!(1_700_000_000_000u64)
        );
    }

    #[test]
    fn rejects_global_payload_without_usd_market_cap() {
        assert!(normalize_coingecko_global(&serde_json::json!({"data": {}})).is_none());
    }
}
