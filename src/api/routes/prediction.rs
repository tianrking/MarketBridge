use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::external::{
    fetch_polymarket_book, fetch_polymarket_books, fetch_polymarket_crypto_markets,
};

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
        })),
        Err(error) => Json(serde_json::json!({
            "source": "polymarket_gamma",
            "gamma_base_url": gamma_base_url,
            "limit": limit,
            "max_offset": max_offset,
            "error": error.to_string(),
            "markets": [],
            "clob_asset_ids": []
        })),
    }
}

pub async fn polymarket_book(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PolymarketBookQuery>,
) -> impl IntoResponse {
    match fetch_polymarket_book(&state.http, &q.token_id).await {
        Ok(book) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "book": book
        })),
        Err(error) => Json(serde_json::json!({
            "source": "polymarket_clob",
            "token_id": q.token_id,
            "error": error.to_string(),
            "book": null
        })),
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
    for (token_id, result) in token_ids.into_iter().zip(results.into_iter()) {
        match result {
            Ok(book) => books.push(book),
            Err(error) => errors.push(serde_json::json!({
                "token_id": token_id,
                "error": error.to_string()
            })),
        }
    }
    Json(serde_json::json!({
        "source": "polymarket_clob",
        "books": books,
        "errors": errors
    }))
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
        return Json(serde_json::json!({
            "source": "polymarket_clob",
            "error": market_response.err().map(|e| e.to_string()).unwrap_or_default(),
            "markets": [],
            "books": [],
            "errors": []
        }));
    };
    let results = fetch_polymarket_books(&state.http, &market_response.clob_asset_ids).await;
    let mut books = Vec::new();
    let mut errors = Vec::new();
    for (token_id, result) in market_response
        .clob_asset_ids
        .iter()
        .cloned()
        .zip(results.into_iter())
    {
        match result {
            Ok(book) => books.push(book),
            Err(error) => errors.push(serde_json::json!({
                "token_id": token_id,
                "error": error.to_string()
            })),
        }
    }
    Json(serde_json::json!({
        "source": "polymarket_clob",
        "markets": market_response.markets,
        "books": books,
        "errors": errors
    }))
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
        return Json(serde_json::json!({
            "source": "polymarket_clob_ws_cache",
            "error": market_response.err().map(|e| e.to_string()).unwrap_or_default(),
            "markets": [],
            "books": []
        }));
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
}

fn parse_csv_vec(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}
