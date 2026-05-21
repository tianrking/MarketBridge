use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::api::error::{multi_status, upstream_error};
use crate::api::utils::parse_csv_vec;
use crate::connectors::prediction::polymarket::{
    PolymarketBatchPriceHistoryRequest, fetch_polymarket_batch_prices_history,
    fetch_polymarket_book, fetch_polymarket_books, fetch_polymarket_crypto_markets,
    fetch_polymarket_last_trade_prices, fetch_polymarket_market_prices, fetch_polymarket_markets,
    fetch_polymarket_midpoints, fetch_polymarket_prices_history, fetch_polymarket_spreads,
};
use crate::domains::prediction::book::envelope_from_polymarket_book;

#[derive(Debug, Deserialize)]
pub struct PolymarketCryptoMarketsQuery {
    gamma_base_url: Option<String>,
    limit: Option<usize>,
    max_offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct PolymarketBookQuery {
    token_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PolymarketBooksQuery {
    token_ids: String,
}

#[derive(Debug, Deserialize)]
pub struct PolymarketPricesQuery {
    token_ids: String,
    sides: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PolymarketPricesHistoryQuery {
    token_id: Option<String>,
    token_ids: Option<String>,
    start_ts: Option<f64>,
    end_ts: Option<f64>,
    interval: Option<String>,
    fidelity: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PredictionBooksQuery {
    token_ids: Option<String>,
    include_stale: Option<bool>,
}

pub async fn polymarket_crypto_markets(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketCryptoMarketsQuery>,
) -> impl IntoResponse {
    let gamma_base_url = q
        .gamma_base_url
        .unwrap_or_else(|| "https://gamma-api.polymarket.com/".to_string());
    let limit = q.limit.unwrap_or(500);
    let max_offset = q.max_offset.unwrap_or(5000);
    match fetch_polymarket_crypto_markets(&state.http, &gamma_base_url, limit, max_offset).await {
        Ok(response) => Json(serde_json::json!({
            "source": "polymarket_gamma",
            "gamma_base_url": gamma_base_url,
            "limit": limit,
            "max_offset": max_offset,
            "markets": response.markets,
            "clob_asset_ids": response.clob_asset_ids
        }))
        .into_response(),
        Err(error) => upstream_error("polymarket_gamma", error),
    }
}

pub async fn polymarket_markets(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketCryptoMarketsQuery>,
) -> impl IntoResponse {
    let gamma_base_url = q
        .gamma_base_url
        .unwrap_or_else(|| "https://gamma-api.polymarket.com/".to_string());
    let limit = q.limit.unwrap_or(500);
    let max_offset = q.max_offset.unwrap_or(5000);
    match fetch_polymarket_markets(&state.http, &gamma_base_url, limit, max_offset).await {
        Ok(response) => Json(serde_json::json!({
            "source": "polymarket_gamma",
            "gamma_base_url": gamma_base_url,
            "limit": limit,
            "max_offset": max_offset,
            "markets": response.markets,
            "clob_asset_ids": response.clob_asset_ids
        }))
        .into_response(),
        Err(error) => upstream_error("polymarket_gamma", error),
    }
}

pub async fn v1_prediction_books(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PredictionBooksQuery>,
) -> impl IntoResponse {
    let include_stale = q.include_stale.unwrap_or(false);
    let rows = if let Some(token_ids) = q.token_ids {
        let token_ids = parse_csv_vec(&token_ids);
        state.polymarket_cache.by_ids(&token_ids).await
    } else {
        state.polymarket_cache.all().await
    };
    let books = rows
        .into_iter()
        .filter(|book| include_stale || !book.stale)
        .map(envelope_from_polymarket_book)
        .collect::<Vec<_>>();

    Json(serde_json::json!({
        "version": "v1",
        "domain": "prediction_book",
        "books": books
    }))
}

pub async fn polymarket_book(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketBookQuery>,
) -> impl IntoResponse {
    match fetch_polymarket_book(&state.http, &q.token_id).await {
        Ok(book) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "book": book
        }))
        .into_response(),
        Err(error) => upstream_error("polymarket_clob", error),
    }
}

pub async fn polymarket_books(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketBooksQuery>,
) -> impl IntoResponse {
    let token_ids = parse_csv_vec(&q.token_ids);
    let results = fetch_polymarket_books(&state.http, &token_ids).await;
    let mut books = Vec::new();
    let mut errors = Vec::new();
    for (token_id, result) in token_ids.into_iter().zip(results) {
        match result {
            Ok(book) => books.push(book),
            Err(error) => errors.push(serde_json::json!({
                "token_id": token_id,
                "error": error.to_string()
            })),
        }
    }
    let body = serde_json::json!({
        "source": "polymarket_clob",
        "books": books,
        "errors": errors
    });
    if errors.is_empty() {
        Json(body).into_response()
    } else {
        multi_status(body)
    }
}

pub async fn polymarket_midpoints(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketBooksQuery>,
) -> impl IntoResponse {
    let token_ids = parse_csv_vec(&q.token_ids);
    match fetch_polymarket_midpoints(&state.http, &token_ids).await {
        Ok(midpoints) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "endpoint": "midpoints",
            "api_key_required": false,
            "midpoints": midpoints
        }))
        .into_response(),
        Err(error) => invalid_or_upstream("polymarket_clob", error),
    }
}

pub async fn polymarket_spreads(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketBooksQuery>,
) -> impl IntoResponse {
    let token_ids = parse_csv_vec(&q.token_ids);
    match fetch_polymarket_spreads(&state.http, &token_ids).await {
        Ok(spreads) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "endpoint": "spreads",
            "api_key_required": false,
            "spreads": spreads
        }))
        .into_response(),
        Err(error) => invalid_or_upstream("polymarket_clob", error),
    }
}

pub async fn polymarket_last_trade_prices(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketBooksQuery>,
) -> impl IntoResponse {
    let token_ids = parse_csv_vec(&q.token_ids);
    match fetch_polymarket_last_trade_prices(&state.http, &token_ids).await {
        Ok(last_trade_prices) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "endpoint": "last-trades-prices",
            "api_key_required": false,
            "last_trade_prices": last_trade_prices
        }))
        .into_response(),
        Err(error) => invalid_or_upstream("polymarket_clob", error),
    }
}

pub async fn polymarket_market_prices(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketPricesQuery>,
) -> impl IntoResponse {
    let token_ids = parse_csv_vec(&q.token_ids);
    let sides = q.sides.map(|s| parse_csv_vec(&s)).unwrap_or_default();
    match fetch_polymarket_market_prices(&state.http, &token_ids, &sides).await {
        Ok(prices) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "endpoint": "prices",
            "api_key_required": false,
            "prices": prices
        }))
        .into_response(),
        Err(error) => invalid_or_upstream("polymarket_clob", error),
    }
}

pub async fn polymarket_prices_history(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketPricesHistoryQuery>,
) -> impl IntoResponse {
    if let Some(token_ids) = q.token_ids {
        let token_ids = parse_csv_vec(&token_ids);
        let request = PolymarketBatchPriceHistoryRequest {
            markets: token_ids,
            start_ts: q.start_ts,
            end_ts: q.end_ts,
            interval: q.interval,
            fidelity: q.fidelity,
        };
        return match fetch_polymarket_batch_prices_history(&state.http, &request).await {
            Ok(history) => Json(serde_json::json!({
                "source": "polymarket_clob",
                "endpoint": "batch-prices-history",
                "api_key_required": false,
                "history": history.history
            }))
            .into_response(),
            Err(error) => invalid_or_upstream("polymarket_clob", error),
        };
    }

    let Some(token_id) = q.token_id else {
        return invalid_request("token_id or token_ids is required");
    };

    match fetch_polymarket_prices_history(
        &state.http,
        &token_id,
        q.start_ts,
        q.end_ts,
        q.interval.as_deref(),
        q.fidelity,
    )
    .await
    {
        Ok(history) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "endpoint": "prices-history",
            "api_key_required": false,
            "token_id": token_id,
            "history": history.history
        }))
        .into_response(),
        Err(error) => invalid_or_upstream("polymarket_clob", error),
    }
}

pub async fn polymarket_crypto_books(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketCryptoMarketsQuery>,
) -> impl IntoResponse {
    let gamma_base_url = q
        .gamma_base_url
        .unwrap_or_else(|| "https://gamma-api.polymarket.com/".to_string());
    let limit = q.limit.unwrap_or(500);
    let max_offset = q.max_offset.unwrap_or(5000);
    let market_response =
        fetch_polymarket_crypto_markets(&state.http, &gamma_base_url, limit, max_offset).await;
    let Ok(market_response) = market_response else {
        return upstream_error(
            "polymarket_gamma",
            market_response
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default(),
        );
    };
    let results = fetch_polymarket_books(&state.http, &market_response.clob_asset_ids).await;
    let mut books = Vec::new();
    let mut errors = Vec::new();
    for (token_id, result) in market_response.clob_asset_ids.iter().cloned().zip(results) {
        match result {
            Ok(book) => books.push(book),
            Err(error) => errors.push(serde_json::json!({
                "token_id": token_id,
                "error": error.to_string()
            })),
        }
    }
    let body = serde_json::json!({
        "source": "polymarket_clob",
        "markets": market_response.markets,
        "books": books,
        "errors": errors
    });
    if errors.is_empty() {
        Json(body).into_response()
    } else {
        multi_status(body)
    }
}

pub async fn polymarket_live_books(
    State(state): State<Arc<ApiState>>,
    q: Option<Query<PolymarketBooksQuery>>,
) -> impl IntoResponse {
    let books = if let Some(Query(q)) = q {
        let token_ids = parse_csv_vec(&q.token_ids);
        state.polymarket_cache.by_ids(&token_ids).await
    } else {
        state.polymarket_cache.all().await
    };
    Json(serde_json::json!({
        "source": "polymarket_clob_ws_cache",
        "books": books
    }))
}

pub async fn polymarket_live_crypto_books(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketCryptoMarketsQuery>,
) -> impl IntoResponse {
    let gamma_base_url = q
        .gamma_base_url
        .unwrap_or_else(|| "https://gamma-api.polymarket.com/".to_string());
    let limit = q.limit.unwrap_or(500);
    let max_offset = q.max_offset.unwrap_or(5000);
    let market_response =
        fetch_polymarket_crypto_markets(&state.http, &gamma_base_url, limit, max_offset).await;
    let Ok(market_response) = market_response else {
        return upstream_error(
            "polymarket_gamma",
            market_response
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default(),
        );
    };
    let books = state
        .polymarket_cache
        .by_ids(&market_response.clob_asset_ids)
        .await;
    Json(serde_json::json!({
        "source": "polymarket_clob_ws_cache",
        "markets": market_response.markets,
        "books": books
    }))
    .into_response()
}

fn invalid_request(error: impl ToString) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "source": "market_bridge",
            "error": error.to_string()
        })),
    )
        .into_response()
}

fn invalid_or_upstream(source: &'static str, error: anyhow::Error) -> axum::response::Response {
    let message = error.to_string();
    if message.contains("at least one")
        || message.contains("at most")
        || message.contains("sides must")
    {
        invalid_request(message)
    } else {
        upstream_error(source, message)
    }
}
