use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::api::ApiState;
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper, parse_csv_vec};
use crate::core::schema::ProductType;
use crate::domains::market::quote::QuotePayload;
use crate::klines::KlineQuery;
use crate::market_discovery::{
    PerpetualFundingQuery, fetch_perpetual_funding, supported_perpetual_funding_exchanges,
};
use crate::order_flow::{FootprintQuery, OrderFlowQuery};
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
    persist: Option<bool>,
    candle_type: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OrderFlowHttpQuery {
    exchange: Option<String>,
    market: Option<String>,
    symbol: Option<String>,
    window_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OrderFlowWindowsHttpQuery {
    exchange: Option<String>,
    market: Option<String>,
    symbol: Option<String>,
    windows_ms: Option<String>,
    limit_per_window: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct FootprintHttpQuery {
    exchange: Option<String>,
    market: Option<String>,
    symbol: Option<String>,
    interval_ms: Option<u64>,
    scale: Option<f64>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    imbalance_ratio: Option<f64>,
    imbalance_volume: Option<f64>,
    stacked_imbalance_range: Option<usize>,
    include_trades: Option<bool>,
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
        .quote_snapshots_matching(|quote| {
            (include_stale || !quote.freshness.stale)
                && symbols.as_ref().is_none_or(|set| {
                    quote
                        .instrument_ref
                        .symbol
                        .as_deref()
                        .is_some_and(|symbol| set.contains(&symbol.to_ascii_uppercase()))
                })
        })
        .await
        .into_iter()
        .filter(|quote| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&quote.source_ref.source.to_ascii_lowercase()))
                && product_type.as_ref().is_none_or(|value| {
                    product_type_label(quote.instrument_ref.product_type)
                        .eq_ignore_ascii_case(value)
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
    let rows = filtered_market_rows(
        state.bus.funding_snapshot_all().await,
        &q,
        |row| &row.symbol,
        |row| row.exchange,
        |_| None,
        |a, b| cmp_symbol_exchange(&a.symbol, a.exchange, &b.symbol, b.exchange),
    );
    Json(serde_json::json!({"version":"v1","domain":"market_funding","funding":rows}))
}

pub async fn v1_market_perpetual_funding(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PerpetualFundingQuery>,
) -> impl IntoResponse {
    let (rows, errors) = fetch_perpetual_funding(&state.http, &q).await;
    Json(serde_json::json!({
        "version":"v1",
        "domain":"market_perpetual_funding",
        "supported_exchanges": supported_perpetual_funding_exchanges(),
        "funding": rows,
        "errors": errors
    }))
}

pub async fn v1_market_open_interest(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let rows = filtered_market_rows(
        state.bus.open_interest_snapshot_all().await,
        &q,
        |row| &row.symbol,
        |row| row.exchange,
        |_| None,
        |a, b| cmp_symbol_exchange(&a.symbol, a.exchange, &b.symbol, b.exchange),
    );
    Json(serde_json::json!({"version":"v1","domain":"market_open_interest","open_interest":rows}))
}

pub async fn v1_market_trades(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let rows = filtered_market_rows(
        state.bus.trade_snapshot_all().await,
        &q,
        |row| &row.symbol,
        |row| row.exchange,
        |row| Some(row.market),
        |a, b| cmp_symbol_exchange(&a.symbol, a.exchange, &b.symbol, b.exchange),
    );
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

pub async fn v1_market_order_flow_windows(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<OrderFlowWindowsHttpQuery>,
) -> impl IntoResponse {
    let windows = parse_windows_ms(q.windows_ms.as_deref());
    let mut rows = Vec::new();
    for window_ms in windows {
        rows.extend(
            state
                .order_flow_store
                .query(OrderFlowQuery {
                    exchange: q.exchange.as_ref().map(|x| x.trim().to_ascii_lowercase()),
                    market: q.market.as_ref().map(|x| x.trim().to_ascii_lowercase()),
                    symbol: q.symbol.as_ref().map(|x| x.trim().to_ascii_uppercase()),
                    window_ms: Some(window_ms),
                    limit: q.limit_per_window.unwrap_or(50),
                })
                .await,
        );
    }
    rows.sort_by(|a, b| {
        a.exchange
            .cmp(&b.exchange)
            .then(a.market.cmp(&b.market))
            .then(a.symbol.cmp(&b.symbol))
            .then(a.window_ms.cmp(&b.window_ms))
            .then(b.bucket_start_ms.cmp(&a.bucket_start_ms))
    });
    Json(serde_json::json!({"version":"v1","domain":"market_order_flow_windows","order_flow":rows}))
}

pub async fn v1_market_footprint(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<FootprintHttpQuery>,
) -> impl IntoResponse {
    let rows = state
        .order_flow_store
        .query_footprint(FootprintQuery {
            exchange: q.exchange.map(|x| x.trim().to_ascii_lowercase()),
            market: q.market.map(|x| x.trim().to_ascii_lowercase()),
            symbol: q.symbol.map(|x| x.trim().to_ascii_uppercase()),
            interval_ms: q.interval_ms.unwrap_or(60_000),
            scale: q.scale.unwrap_or(1.0),
            start_ms: q.start_ms,
            end_ms: q.end_ms,
            imbalance_ratio: q.imbalance_ratio.unwrap_or(3.0),
            imbalance_volume: q.imbalance_volume.unwrap_or(0.0),
            stacked_imbalance_range: q.stacked_imbalance_range.unwrap_or(3),
            include_trades: q.include_trades.unwrap_or(false),
            limit: q.limit.unwrap_or(100),
        })
        .await;
    Json(serde_json::json!({"version":"v1","domain":"market_footprint","footprints":rows}))
}

pub async fn v1_market_klines(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<KlinesQuery>,
) -> impl IntoResponse {
    let persist = q.persist.unwrap_or(false);
    let candle_type = q
        .candle_type
        .clone()
        .or_else(|| q.market.clone())
        .unwrap_or_else(|| "spot".to_string());
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
            let persist_result = if persist {
                match state
                    .data_lake_store
                    .persist_klines(rows.clone(), candle_type)
                    .await
                {
                    Ok(partitions) => serde_json::json!({"ok": true, "partitions": partitions}),
                    Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
                }
            } else {
                serde_json::json!({"ok": false, "reason": "persist_query_param_not_set"})
            };
            Json(
                serde_json::json!({"version":"v1","domain":"market_kline","persist":persist_result,"klines":rows}),
            )
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
    let rows = filtered_market_rows(
        state.bus.liquidation_snapshot_all().await,
        &q,
        |row| &row.symbol,
        |row| row.exchange,
        |_| None,
        |a, b| cmp_symbol_exchange(&a.symbol, a.exchange, &b.symbol, b.exchange),
    );
    Json(serde_json::json!({"version":"v1","domain":"market_liquidation","liquidations":rows}))
}

pub async fn v1_market_order_books(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDataQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.as_ref().cloned().map(parse_csv_set_upper);
    let exchanges = q.exchanges.as_ref().cloned().map(parse_csv_set_lower);
    let market = q.market.as_ref().map(|x| x.trim().to_ascii_lowercase());
    let mut rows = state
        .bus
        .order_book_snapshots_matching(|row| {
            symbols
                .as_ref()
                .is_none_or(|set| set.contains(&row.symbol.to_ascii_uppercase()))
                && exchanges
                    .as_ref()
                    .is_none_or(|set| set.contains(&row.exchange.to_ascii_lowercase()))
                && market
                    .as_ref()
                    .is_none_or(|value| market_kind_label(row.market) == value)
        })
        .await;
    rows.sort_by(|a, b| cmp_symbol_exchange(&a.symbol, a.exchange, &b.symbol, b.exchange));
    Json(serde_json::json!({"version":"v1","domain":"market_order_book","books":rows}))
}

fn filtered_market_rows<T, Symbol, Exchange, Market, Sort>(
    rows: Vec<T>,
    query: &MarketDataQuery,
    symbol_of: Symbol,
    exchange_of: Exchange,
    market_of: Market,
    sort_by: Sort,
) -> Vec<T>
where
    Symbol: Fn(&T) -> &str,
    Exchange: Fn(&T) -> &str,
    Market: Fn(&T) -> Option<crate::types::MarketKind>,
    Sort: Fn(&T, &T) -> Ordering,
{
    let symbols = query.symbols.as_ref().cloned().map(parse_csv_set_upper);
    let exchanges = query.exchanges.as_ref().cloned().map(parse_csv_set_lower);
    let market = query.market.as_ref().map(|x| x.trim().to_ascii_lowercase());
    let mut filtered = rows
        .into_iter()
        .filter(|row| {
            symbols
                .as_ref()
                .is_none_or(|set| set.contains(&symbol_of(row).to_ascii_uppercase()))
        })
        .filter(|row| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&exchange_of(row).to_ascii_lowercase()))
        })
        .filter(|row| {
            market.as_ref().is_none_or(|value| {
                market_of(row).is_none_or(|kind| market_kind_label(kind) == value)
            })
        })
        .collect::<Vec<_>>();
    filtered.sort_by(sort_by);
    filtered
}

fn cmp_symbol_exchange(
    left_symbol: &str,
    left_exchange: &str,
    right_symbol: &str,
    right_exchange: &str,
) -> Ordering {
    left_symbol
        .cmp(right_symbol)
        .then(left_exchange.cmp(right_exchange))
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

fn parse_windows_ms(raw: Option<&str>) -> Vec<u64> {
    raw.map(parse_csv_vec)
        .unwrap_or_else(|| {
            vec![
                "60000".to_string(),
                "300000".to_string(),
                "900000".to_string(),
            ]
        })
        .into_iter()
        .filter_map(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .collect()
}
