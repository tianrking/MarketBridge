use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::api::ApiState;
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};
use crate::core::schema::ProductType;
use crate::domains::market::quote::QuotePayload;
use crate::klines::KlineQuery;
use crate::order_flow::OrderFlowQuery;
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

#[derive(Debug, Deserialize, Default)]
pub struct OrderFlowHttpQuery {
    exchange: Option<String>,
    market: Option<String>,
    symbol: Option<String>,
    window_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct BasisRow {
    exchange: String,
    symbol: String,
    spot_mid: f64,
    perp_mid: f64,
    basis: f64,
    basis_bps: f64,
    spot_ts_ms: u64,
    perp_ts_ms: u64,
    stale: bool,
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

pub async fn v1_market_basis(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let include_stale = false;
    let quotes = state.bus.quote_snapshot_all().await;
    let mut spot = std::collections::HashMap::<(String, String), (f64, u64, bool)>::new();
    let mut perp = std::collections::HashMap::<(String, String), (f64, u64, bool)>::new();

    for quote in quotes {
        let Some(symbol) = quote.instrument_ref.symbol.as_deref() else {
            continue;
        };
        if symbols
            .as_ref()
            .is_some_and(|set| !set.contains(&symbol.to_ascii_uppercase()))
        {
            continue;
        }
        let exchange = quote.source_ref.source.to_ascii_lowercase();
        if exchanges
            .as_ref()
            .is_some_and(|set| !set.contains(&exchange))
        {
            continue;
        }
        if !include_stale && quote.freshness.stale {
            continue;
        }
        let Some(mid) = quote_mid(&quote.payload) else {
            continue;
        };
        let key = (exchange, symbol.to_ascii_uppercase());
        let value = (mid, quote.freshness.ts_source, quote.freshness.stale);
        match quote.instrument_ref.product_type {
            ProductType::Spot => {
                spot.insert(key, value);
            }
            ProductType::Perp => {
                perp.insert(key, value);
            }
            _ => {}
        }
    }

    let mut rows = Vec::new();
    for (key, (spot_mid, spot_ts_ms, spot_stale)) in spot {
        let Some((perp_mid, perp_ts_ms, perp_stale)) = perp.get(&key).copied() else {
            continue;
        };
        let basis = perp_mid - spot_mid;
        rows.push(BasisRow {
            exchange: key.0,
            symbol: key.1,
            spot_mid,
            perp_mid,
            basis,
            basis_bps: basis / spot_mid * 10_000.0,
            spot_ts_ms,
            perp_ts_ms,
            stale: spot_stale || perp_stale,
        });
    }

    rows.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.exchange.cmp(&b.exchange)));
    Json(serde_json::json!({"version":"v1","domain":"market_basis","basis":rows}))
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

pub async fn v1_market_order_flow(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<OrderFlowHttpQuery>,
) -> impl IntoResponse {
    let rows = state
        .order_flow_store
        .query(OrderFlowQuery {
            exchange: q.exchange.map(|x| x.trim().to_ascii_lowercase()),
            market: q.market.map(|x| x.trim().to_ascii_lowercase()),
            symbol: q.symbol.map(|x| x.trim().to_ascii_uppercase()),
            window_ms: q.window_ms,
            limit: q.limit.unwrap_or(500),
        })
        .await;
    Json(serde_json::json!({"version":"v1","domain":"market_order_flow","order_flow":rows}))
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

fn quote_mid(payload: &QuotePayload) -> Option<f64> {
    if payload.bid.is_finite() && payload.ask.is_finite() && payload.bid > 0.0 && payload.ask > 0.0
    {
        Some((payload.bid + payload.ask) / 2.0)
    } else {
        None
    }
}

fn market_kind_label(market: crate::types::MarketKind) -> &'static str {
    match market {
        crate::types::MarketKind::Spot => "spot",
        crate::types::MarketKind::Perp => "perp",
    }
}
