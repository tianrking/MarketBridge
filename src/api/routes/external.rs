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

#[derive(Debug, Deserialize, Default)]
pub struct StablecoinQuery {
    symbols: Option<String>,
    peg_type: Option<String>,
    limit: Option<usize>,
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

pub async fn v1_external_stablecoins(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<StablecoinQuery>,
) -> impl IntoResponse {
    let response = state
        .http
        .get("https://stablecoins.llama.fi/stablecoins?includePrices=true")
        .send()
        .await
        .and_then(|response| response.error_for_status());
    match response {
        Ok(response) => match response.json::<Value>().await {
            Ok(payload) => match normalize_defillama_stablecoins(&payload, &q) {
                Some(data) => Json(serde_json::json!({
                    "version": "v1",
                    "domain": "stablecoin_liquidity_context",
                    "source": "defillama_stablecoins",
                    "coverage": "provider_snapshot",
                    "data": data,
                    "limitations": [
                        "circulating supply is a provider aggregate and does not identify exchange balances or deployable trading liquidity",
                        "chain allocation and supply changes are context observations, not a price or flow forecast",
                        "no order, wallet, signing or execution path is included"
                    ]
                }))
                .into_response(),
                None => (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({
                        "version": "v1",
                        "domain": "stablecoin_liquidity_context",
                        "source": "defillama_stablecoins",
                        "error": "DefiLlama stablecoin payload missing usable peggedAssets"
                    })),
                )
                    .into_response(),
            },
            Err(error) => upstream_error("defillama_stablecoins", error),
        },
        Err(error) => upstream_error("defillama_stablecoins", error),
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

fn normalize_defillama_stablecoins(payload: &Value, query: &StablecoinQuery) -> Option<Value> {
    let symbols = query
        .symbols
        .as_deref()
        .map(|value| parse_csv_set_upper(value.to_string()));
    let peg_type = query
        .peg_type
        .as_deref()
        .unwrap_or("peggedUSD")
        .trim()
        .to_ascii_lowercase();
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let assets = payload
        .get("peggedAssets")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|row| {
            let symbol = row.get("symbol")?.as_str()?.to_ascii_uppercase();
            let row_peg_type = row.get("pegType")?.as_str()?;
            if !peg_type.is_empty() && !row_peg_type.eq_ignore_ascii_case(&peg_type) {
                return None;
            }
            if symbols.as_ref().is_some_and(|set| !set.contains(&symbol)) {
                return None;
            }
            let current = row
                .pointer("/circulating/peggedUSD")
                .and_then(value_f64_value)?;
            let previous_day = row
                .pointer("/circulatingPrevDay/peggedUSD")
                .and_then(value_f64_value);
            let previous_week = row
                .pointer("/circulatingPrevWeek/peggedUSD")
                .and_then(value_f64_value);
            let previous_month = row
                .pointer("/circulatingPrevMonth/peggedUSD")
                .and_then(value_f64_value);
            Some(serde_json::json!({
                "id": row.get("id"),
                "name": row.get("name"),
                "symbol": symbol,
                "peg_type": row.get("pegType"),
                "peg_mechanism": row.get("pegMechanism"),
                "current_supply_usd": current,
                "previous_day_supply_usd": previous_day,
                "previous_week_supply_usd": previous_week,
                "previous_month_supply_usd": previous_month,
                "change_24h_pct": pct_change(current, previous_day),
                "change_7d_pct": pct_change(current, previous_week),
                "change_30d_pct": pct_change(current, previous_month),
                "chain_count": row.get("chainCirculating").and_then(Value::as_object).map(|chains| chains.len())
            }))
        })
        .collect::<Vec<_>>();
    let mut assets = assets;
    let total_supply_usd = assets
        .iter()
        .filter_map(|row| row.get("current_supply_usd").and_then(Value::as_f64))
        .sum::<f64>();
    assets.sort_by(|a, b| {
        b.get("current_supply_usd")
            .and_then(Value::as_f64)
            .partial_cmp(&a.get("current_supply_usd").and_then(Value::as_f64))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.get("symbol")
                    .and_then(Value::as_str)
                    .cmp(&b.get("symbol").and_then(Value::as_str))
            })
    });
    assets.truncate(limit);
    let chains = payload
        .get("chains")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some(serde_json::json!({
                "name": row.get("name")?,
                "supply_usd": row.pointer("/totalCirculatingUSD/peggedUSD").and_then(value_f64_value)
            }))
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({
        "peg_type": peg_type,
        "total_supply_usd": total_supply_usd,
        "assets": assets,
        "chains": chains,
        "updated_at_ms": crate::types::now_ms(),
        "source": "defillama_stablecoins"
    }))
}

fn value_f64_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())
        .filter(|number| number.is_finite())
}

fn pct_change(current: f64, previous: Option<f64>) -> Option<f64> {
    previous
        .filter(|value| *value > 0.0)
        .map(|value| (current / value - 1.0) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::{StablecoinQuery, normalize_coingecko_global, normalize_defillama_stablecoins};

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

    #[test]
    fn normalizes_stablecoin_supply_and_chain_context() {
        let payload = serde_json::json!({
            "peggedAssets": [
                {"id":"1", "name":"Tether", "symbol":"USDT", "pegType":"peggedUSD",
                 "pegMechanism":"fiat-backed", "circulating":{"peggedUSD":"120"},
                 "circulatingPrevDay":{"peggedUSD":"100"}, "chainCirculating":{"Ethereum":{}}},
                {"id":"2", "name":"Other", "symbol":"OTHER", "pegType":"peggedEUR",
                 "circulating":{"peggedUSD":"50"}}
            ],
            "chains": [{"name":"Ethereum", "totalCirculatingUSD":{"peggedUSD":"80"}}]
        });
        let query = StablecoinQuery {
            symbols: Some("USDT".to_string()),
            ..Default::default()
        };
        let data =
            normalize_defillama_stablecoins(&payload, &query).expect("normalized stablecoins");
        assert_eq!(data["assets"][0]["symbol"], serde_json::json!("USDT"));
        let change = data["assets"][0]["change_24h_pct"]
            .as_f64()
            .expect("numeric stablecoin change");
        assert!((change - 20.0).abs() < 1e-9);
        assert_eq!(data["chains"][0]["supply_usd"], serde_json::json!(80.0));
    }
}
