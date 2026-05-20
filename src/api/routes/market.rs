use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};
use crate::core::schema::ProductType;
use crate::klines::KlineQuery;
#[derive(Debug, Deserialize, Default)]
pub struct MarketQuotesQuery {
    symbols: Option<String>,
    exchanges: Option<String>,
    product_type: Option<String>,
    include_stale: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MarketDataQuery {
    symbols: Option<String>,
    exchanges: Option<String>,
    market: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct KlinesQuery {
    exchange: Option<String>,
    market: Option<String>,
    symbol: Option<String>,
    interval: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
}

pub async fn v1_market_quotes(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketQuotesQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let product_type = q.product_type.map(|x| x.trim().to_ascii_lowercase());
    let include_stale = q.include_stale.unwrap_or(false);

    let mut quotes = state
        .bus
        .quote_snapshot_all()
        .await
        .into_iter()
        .filter(|quote| include_stale || !quote.freshness.stale)
        .filter(|quote| {
            symbols.as_ref().is_none_or(|set| {
                quote
                    .instrument_ref
                    .symbol
                    .as_deref()
                    .is_some_and(|symbol| set.contains(&symbol.to_ascii_uppercase()))
            })
        })
        .filter(|quote| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&quote.source_ref.source.to_ascii_lowercase()))
        })
        .filter(|quote| {
            product_type.as_ref().is_none_or(|value| {
                product_type_label(quote.instrument_ref.product_type).eq_ignore_ascii_case(value)
            })
        })
        .collect::<Vec<_>>();

    quotes.sort_by(|a, b| {
        a.instrument_ref
            .instrument_id
            .cmp(&b.instrument_ref.instrument_id)
            .then_with(|| a.source_ref.source.cmp(&b.source_ref.source))
    });

    Json(serde_json::json!({
        "version": "v1",
        "domain": "market_quote",
        "quotes": quotes
    }))
}

pub async fn v1_market_funding(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let mut rows = state
        .bus
        .funding_snapshot_all()
        .await
        .into_iter()
        .filter(|row| {
            symbols
                .as_ref()
                .is_none_or(|set| set.contains(&row.symbol.to_ascii_uppercase()))
        })
        .filter(|row| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&row.exchange.to_ascii_lowercase()))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.exchange.cmp(b.exchange)));
    Json(serde_json::json!({"version":"v1","domain":"market_funding","funding":rows}))
}

pub async fn v1_market_open_interest(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let mut rows = state
        .bus
        .open_interest_snapshot_all()
        .await
        .into_iter()
        .filter(|row| {
            symbols
                .as_ref()
                .is_none_or(|set| set.contains(&row.symbol.to_ascii_uppercase()))
        })
        .filter(|row| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&row.exchange.to_ascii_lowercase()))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.exchange.cmp(b.exchange)));
    Json(serde_json::json!({"version":"v1","domain":"market_open_interest","open_interest":rows}))
}

pub async fn v1_market_trades(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let market = q.market.map(|x| x.trim().to_ascii_lowercase());
    let mut rows = state
        .bus
        .trade_snapshot_all()
        .await
        .into_iter()
        .filter(|row| {
            symbols
                .as_ref()
                .is_none_or(|set| set.contains(&row.symbol.to_ascii_uppercase()))
        })
        .filter(|row| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&row.exchange.to_ascii_lowercase()))
        })
        .filter(|row| {
            market
                .as_ref()
                .is_none_or(|value| market_kind_label(row.market) == value)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.exchange.cmp(b.exchange)));
    Json(serde_json::json!({"version":"v1","domain":"market_trade","trades":rows}))
}

pub async fn v1_market_klines(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<KlinesQuery>,
) -> impl IntoResponse {
    let query = KlineQuery {
        exchange: q.exchange.map(|x| x.trim().to_ascii_lowercase()),
        market: q.market.map(|x| x.trim().to_ascii_lowercase()),
        symbol: q.symbol.map(|x| x.trim().to_ascii_uppercase()),
        interval: q.interval.map(|x| x.trim().to_string()),
        start_ms: q.start_ms,
        end_ms: q.end_ms,
        limit: q.limit.unwrap_or(500),
    };
    match state.kline_store.query(query).await {
        Ok(mut rows) => {
            rows.sort_by(|a, b| {
                a.exchange
                    .cmp(&b.exchange)
                    .then(a.market.cmp(&b.market))
                    .then(a.symbol.cmp(&b.symbol))
                    .then(a.interval.cmp(&b.interval))
                    .then(a.open_time_ms.cmp(&b.open_time_ms))
            });
            Json(serde_json::json!({"version":"v1","domain":"market_kline","klines":rows}))
        }
        Err(error) => Json(serde_json::json!({
            "version":"v1",
            "domain":"market_kline",
            "error": error.to_string(),
            "klines": []
        })),
    }
}

pub async fn v1_market_liquidations(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let mut rows = state
        .bus
        .liquidation_snapshot_all()
        .await
        .into_iter()
        .filter(|row| {
            symbols
                .as_ref()
                .is_none_or(|set| set.contains(&row.symbol.to_ascii_uppercase()))
        })
        .filter(|row| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&row.exchange.to_ascii_lowercase()))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.exchange.cmp(b.exchange)));
    Json(serde_json::json!({"version":"v1","domain":"market_liquidation","liquidations":rows}))
}

pub async fn v1_market_order_books(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let market = q.market.map(|x| x.trim().to_ascii_lowercase());
    let mut rows = state
        .bus
        .order_book_snapshot_all()
        .await
        .into_iter()
        .filter(|row| {
            symbols
                .as_ref()
                .is_none_or(|set| set.contains(&row.symbol.to_ascii_uppercase()))
        })
        .filter(|row| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&row.exchange.to_ascii_lowercase()))
        })
        .filter(|row| {
            market
                .as_ref()
                .is_none_or(|value| market_kind_label(row.market) == value)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.exchange.cmp(b.exchange)));
    Json(serde_json::json!({"version":"v1","domain":"market_order_book","books":rows}))
}

fn product_type_label(product_type: ProductType) -> &'static str {
    match product_type {
        ProductType::Spot => "spot",
        ProductType::Perp => "perp",
        ProductType::Future => "future",
        ProductType::Option => "option",
        ProductType::BinaryOutcome => "binary_outcome",
        ProductType::WalletTransfer => "wallet_transfer",
        ProductType::DexPool => "dex_pool",
        ProductType::Event => "event",
    }
}

fn market_kind_label(market: crate::types::MarketKind) -> &'static str {
    match market {
        crate::types::MarketKind::Spot => "spot",
        crate::types::MarketKind::Perp => "perp",
    }
}
