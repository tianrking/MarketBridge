use std::collections::{BTreeMap, HashMap, HashSet};
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
use crate::types::ExternalSignalTick;

#[derive(Debug, Deserialize, Default)]
pub struct UniverseKlineQuery {
    exchange: Option<String>,
    market: Option<String>,
    symbols: Option<String>,
    interval: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    min_value: Option<f64>,
    max_value: Option<f64>,
    sort: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UniverseSpreadQuery {
    symbols: Option<String>,
    exchanges: Option<String>,
    product_type: Option<String>,
    max_spread_bps: Option<f64>,
    include_stale: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UniverseCrossMarketQuery {
    symbols: Option<String>,
    exchanges: Option<String>,
    include_stale: Option<bool>,
    require_both: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UniverseExternalQuery {
    symbols: Option<String>,
    sources: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct VolumeRow {
    exchange: String,
    market: String,
    symbol: String,
    interval: String,
    bars: usize,
    base_volume: f64,
    quote_volume: f64,
    first_open_time_ms: u64,
    last_open_time_ms: u64,
}

#[derive(Debug, Serialize)]
struct PercentChangeRow {
    exchange: String,
    market: String,
    symbol: String,
    interval: String,
    bars: usize,
    first_close: f64,
    last_close: f64,
    percent_change: f64,
    first_open_time_ms: u64,
    last_open_time_ms: u64,
}

#[derive(Debug, Serialize)]
struct VolatilityRow {
    exchange: String,
    market: String,
    symbol: String,
    interval: String,
    bars: usize,
    realized_volatility: f64,
    first_open_time_ms: u64,
    last_open_time_ms: u64,
}

#[derive(Debug, Serialize)]
struct SpreadRow {
    exchange: String,
    product_type: String,
    symbol: String,
    bid: f64,
    ask: f64,
    mid: f64,
    spread: f64,
    spread_bps: f64,
    stale: bool,
    ts_source: u64,
}

#[derive(Debug, Serialize)]
struct CrossMarketRow {
    symbol: String,
    spot_sources: Vec<String>,
    perp_sources: Vec<String>,
    has_spot: bool,
    has_perp: bool,
    has_both: bool,
}

#[derive(Debug, Serialize)]
struct MarketCapRow {
    source: String,
    symbol: String,
    market_cap: f64,
    rank: usize,
    ts_ms: u64,
}

#[derive(Debug, Serialize)]
struct ExternalUniverseRow {
    source: String,
    category: String,
    symbol: Option<String>,
    metric: String,
    value: Option<f64>,
    score: Option<f64>,
    title: Option<String>,
    url: Option<String>,
    ts_ms: u64,
}

#[derive(Default)]
struct KlineGroup {
    exchange: String,
    market: String,
    symbol: String,
    interval: String,
    bars: Vec<crate::klines::KlineBar>,
}

pub async fn top_volume(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseKlineQuery>,
) -> impl IntoResponse {
    let rows = load_kline_groups(&state, &q)
        .await
        .into_iter()
        .filter_map(volume_row)
        .filter(|row| within(row.quote_volume, q.min_value, q.max_value))
        .collect::<Vec<_>>();
    let rows = sort_and_limit(rows, q.sort.as_deref(), q.limit.unwrap_or(100), |row| {
        row.quote_volume
    });
    Json(serde_json::json!({"version":"v1","domain":"universe_top_volume","rows":rows}))
}

pub async fn percent_change(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseKlineQuery>,
) -> impl IntoResponse {
    let rows = load_kline_groups(&state, &q)
        .await
        .into_iter()
        .filter_map(percent_change_row)
        .filter(|row| within(row.percent_change, q.min_value, q.max_value))
        .collect::<Vec<_>>();
    let rows = sort_and_limit(rows, q.sort.as_deref(), q.limit.unwrap_or(100), |row| {
        row.percent_change
    });
    Json(serde_json::json!({"version":"v1","domain":"universe_percent_change","rows":rows}))
}

pub async fn volatility(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseKlineQuery>,
) -> impl IntoResponse {
    let rows = load_kline_groups(&state, &q)
        .await
        .into_iter()
        .filter_map(volatility_row)
        .filter(|row| within(row.realized_volatility, q.min_value, q.max_value))
        .collect::<Vec<_>>();
    let rows = sort_and_limit(rows, q.sort.as_deref(), q.limit.unwrap_or(100), |row| {
        row.realized_volatility
    });
    Json(serde_json::json!({"version":"v1","domain":"universe_volatility","rows":rows}))
}

pub async fn spread_filter(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseSpreadQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let product_type = q.product_type.map(|x| x.trim().to_ascii_lowercase());
    let include_stale = q.include_stale.unwrap_or(false);
    let mut rows = state
        .bus
        .quote_snapshot_all()
        .await
        .into_iter()
        .filter(|quote| include_stale || !quote.freshness.stale)
        .filter(|quote| quote_matches(quote, &symbols, &exchanges, product_type.as_deref()))
        .filter_map(spread_row)
        .filter(|row| q.max_spread_bps.is_none_or(|max| row.spread_bps <= max))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        a.spread_bps
            .total_cmp(&b.spread_bps)
            .then(a.symbol.cmp(&b.symbol))
    });
    rows.truncate(q.limit.unwrap_or(100).clamp(1, 1000));
    Json(serde_json::json!({"version":"v1","domain":"universe_spread_filter","rows":rows}))
}

pub async fn cross_market(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseCrossMarketQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let include_stale = q.include_stale.unwrap_or(false);
    let require_both = q.require_both.unwrap_or(true);
    let mut grouped: BTreeMap<String, (HashSet<String>, HashSet<String>)> = BTreeMap::new();
    for quote in state.bus.quote_snapshot_all().await {
        if !include_stale && quote.freshness.stale {
            continue;
        }
        if !quote_matches(&quote, &symbols, &exchanges, None) {
            continue;
        }
        let Some(symbol) = quote.instrument_ref.symbol.as_deref() else {
            continue;
        };
        let entry = grouped
            .entry(symbol.to_ascii_uppercase())
            .or_insert_with(|| (HashSet::new(), HashSet::new()));
        match quote.instrument_ref.product_type {
            ProductType::Spot => {
                entry.0.insert(quote.source_ref.source);
            }
            ProductType::Perp => {
                entry.1.insert(quote.source_ref.source);
            }
            _ => {}
        }
    }

    let mut rows = grouped
        .into_iter()
        .map(|(symbol, (spot, perp))| {
            let mut spot_sources = spot.into_iter().collect::<Vec<_>>();
            let mut perp_sources = perp.into_iter().collect::<Vec<_>>();
            spot_sources.sort();
            perp_sources.sort();
            let has_spot = !spot_sources.is_empty();
            let has_perp = !perp_sources.is_empty();
            CrossMarketRow {
                symbol,
                spot_sources,
                perp_sources,
                has_spot,
                has_perp,
                has_both: has_spot && has_perp,
            }
        })
        .filter(|row| !require_both || row.has_both)
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    rows.truncate(q.limit.unwrap_or(100).clamp(1, 1000));
    Json(serde_json::json!({"version":"v1","domain":"universe_cross_market","rows":rows}))
}

pub async fn market_cap(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseExternalQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let sources = q.sources.map(parse_csv_set_lower);
    let mut rows = state
        .bus
        .external_signal_snapshot_all()
        .await
        .into_iter()
        .filter(|signal| external_matches(signal, &symbols, &sources))
        .filter(|signal| signal.metric.eq_ignore_ascii_case("market_cap"))
        .filter_map(|signal| {
            Some(MarketCapRow {
                source: signal.source.to_string(),
                symbol: signal.symbol.as_deref()?.to_string(),
                market_cap: signal.value?,
                rank: 0,
                ts_ms: signal.ts_ms,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.market_cap.total_cmp(&a.market_cap));
    for (idx, row) in rows.iter_mut().enumerate() {
        row.rank = idx + 1;
    }
    rows.truncate(q.limit.unwrap_or(100).clamp(1, 1000));
    Json(serde_json::json!({"version":"v1","domain":"universe_market_cap","rows":rows}))
}

pub async fn new_listings(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseExternalQuery>,
) -> impl IntoResponse {
    let rows = external_universe_rows(&state, &q, "listing").await;
    Json(serde_json::json!({"version":"v1","domain":"universe_new_listings","rows":rows}))
}

pub async fn delist_risk(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<UniverseExternalQuery>,
) -> impl IntoResponse {
    let rows = external_universe_rows(&state, &q, "delist").await;
    Json(serde_json::json!({"version":"v1","domain":"universe_delist_risk","rows":rows}))
}

async fn load_kline_groups(state: &ApiState, q: &UniverseKlineQuery) -> Vec<KlineGroup> {
    let symbols = q.symbols.as_ref().cloned().map(parse_csv_set_upper);
    let query = KlineQuery {
        exchange: q.exchange.as_ref().map(|x| x.trim().to_ascii_lowercase()),
        market: q.market.as_ref().map(|x| x.trim().to_ascii_lowercase()),
        symbol: None,
        interval: q
            .interval
            .as_ref()
            .map(|x| x.trim().to_string())
            .or_else(|| Some("1d".to_string())),
        start_ms: q.start_ms,
        end_ms: q.end_ms,
        limit: q.limit.unwrap_or(5000).clamp(1, 5000),
    };
    let Ok(rows) = state.kline_store.query(query).await else {
        return Vec::new();
    };
    let mut grouped = HashMap::<String, KlineGroup>::new();
    for row in rows {
        if symbols
            .as_ref()
            .is_some_and(|set| !set.contains(&row.symbol.to_ascii_uppercase()))
        {
            continue;
        }
        let key = format!(
            "{}:{}:{}:{}",
            row.exchange, row.market, row.symbol, row.interval
        );
        let group = grouped.entry(key).or_insert_with(|| KlineGroup {
            exchange: row.exchange.clone(),
            market: row.market.clone(),
            symbol: row.symbol.clone(),
            interval: row.interval.clone(),
            bars: Vec::new(),
        });
        group.bars.push(row);
    }
    grouped
        .into_values()
        .map(|mut group| {
            group.bars.sort_by_key(|bar| bar.open_time_ms);
            group
        })
        .collect()
}

fn volume_row(group: KlineGroup) -> Option<VolumeRow> {
    let first_open_time_ms = group.bars.first()?.open_time_ms;
    let last_open_time_ms = group.bars.last()?.open_time_ms;
    let mut base_volume = 0.0;
    let mut quote_volume = 0.0;
    for bar in &group.bars {
        let Some(volume) = bar.volume else {
            continue;
        };
        let typical_price = (bar.open + bar.high + bar.low) / 3.0;
        if typical_price.is_finite() && volume.is_finite() {
            base_volume += volume;
            quote_volume += typical_price * volume;
        }
    }
    Some(VolumeRow {
        exchange: group.exchange,
        market: group.market,
        symbol: group.symbol,
        interval: group.interval,
        bars: group.bars.len(),
        base_volume,
        quote_volume,
        first_open_time_ms,
        last_open_time_ms,
    })
}

fn percent_change_row(group: KlineGroup) -> Option<PercentChangeRow> {
    let first = group.bars.first()?;
    let last = group.bars.last()?;
    if first.close <= 0.0 {
        return None;
    }
    Some(PercentChangeRow {
        exchange: group.exchange,
        market: group.market,
        symbol: group.symbol,
        interval: group.interval,
        bars: group.bars.len(),
        first_close: first.close,
        last_close: last.close,
        percent_change: (last.close - first.close) / first.close * 100.0,
        first_open_time_ms: first.open_time_ms,
        last_open_time_ms: last.open_time_ms,
    })
}

fn volatility_row(group: KlineGroup) -> Option<VolatilityRow> {
    let first_open_time_ms = group.bars.first()?.open_time_ms;
    let last_open_time_ms = group.bars.last()?.open_time_ms;
    let returns = group
        .bars
        .windows(2)
        .filter_map(|window| {
            let prev = window[0].close;
            let next = window[1].close;
            (prev > 0.0 && next > 0.0).then(|| (next / prev).ln())
        })
        .collect::<Vec<_>>();
    if returns.is_empty() {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance =
        returns.iter().map(|ret| (ret - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    Some(VolatilityRow {
        exchange: group.exchange,
        market: group.market,
        symbol: group.symbol,
        interval: group.interval,
        bars: group.bars.len(),
        realized_volatility: variance.sqrt() * (returns.len() as f64).sqrt(),
        first_open_time_ms,
        last_open_time_ms,
    })
}

fn spread_row(quote: crate::core::schema::DataEnvelope<QuotePayload>) -> Option<SpreadRow> {
    let bid = quote.payload.bid;
    let ask = quote.payload.ask;
    if !bid.is_finite() || !ask.is_finite() || bid <= 0.0 || ask <= 0.0 || ask < bid {
        return None;
    }
    let mid = (bid + ask) / 2.0;
    let spread = ask - bid;
    Some(SpreadRow {
        exchange: quote.source_ref.source,
        product_type: product_type_label(quote.instrument_ref.product_type).to_string(),
        symbol: quote.instrument_ref.symbol?,
        bid,
        ask,
        mid,
        spread,
        spread_bps: spread / mid * 10_000.0,
        stale: quote.freshness.stale,
        ts_source: quote.freshness.ts_source,
    })
}

fn quote_matches(
    quote: &crate::core::schema::DataEnvelope<QuotePayload>,
    symbols: &Option<HashSet<String>>,
    exchanges: &Option<HashSet<String>>,
    product_type: Option<&str>,
) -> bool {
    symbols.as_ref().is_none_or(|set| {
        quote
            .instrument_ref
            .symbol
            .as_deref()
            .is_some_and(|symbol| set.contains(&symbol.to_ascii_uppercase()))
    }) && exchanges
        .as_ref()
        .is_none_or(|set| set.contains(&quote.source_ref.source.to_ascii_lowercase()))
        && product_type.is_none_or(|value| {
            product_type_label(quote.instrument_ref.product_type).eq_ignore_ascii_case(value)
        })
}

async fn external_universe_rows(
    state: &ApiState,
    q: &UniverseExternalQuery,
    needle: &str,
) -> Vec<ExternalUniverseRow> {
    let symbols = q.symbols.as_ref().cloned().map(parse_csv_set_upper);
    let sources = q.sources.as_ref().cloned().map(parse_csv_set_lower);
    let mut rows = state
        .bus
        .external_signal_snapshot_all()
        .await
        .into_iter()
        .filter(|signal| external_matches(signal, &symbols, &sources))
        .filter(|signal| {
            signal.category.to_ascii_lowercase().contains(needle)
                || signal.metric.to_ascii_lowercase().contains(needle)
        })
        .map(|signal| ExternalUniverseRow {
            source: signal.source.to_string(),
            category: signal.category.to_string(),
            symbol: signal.symbol.map(|symbol| symbol.to_string()),
            metric: signal.metric.to_string(),
            value: signal.value,
            score: signal.score,
            title: signal.title.map(|title| title.to_string()),
            url: signal.url.map(|url| url.to_string()),
            ts_ms: signal.ts_ms,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms).then(a.source.cmp(&b.source)));
    rows.truncate(q.limit.unwrap_or(100).clamp(1, 1000));
    rows
}

fn external_matches(
    signal: &ExternalSignalTick,
    symbols: &Option<HashSet<String>>,
    sources: &Option<HashSet<String>>,
) -> bool {
    symbols.as_ref().is_none_or(|set| {
        signal
            .symbol
            .as_deref()
            .is_some_and(|symbol| set.contains(&symbol.to_ascii_uppercase()))
    }) && sources
        .as_ref()
        .is_none_or(|set| set.contains(&signal.source.to_ascii_lowercase()))
}

fn sort_and_limit<T>(
    mut rows: Vec<T>,
    sort: Option<&str>,
    limit: usize,
    value: impl Fn(&T) -> f64,
) -> Vec<T> {
    let descending = !matches!(sort, Some("asc"));
    rows.sort_by(|a, b| {
        let ordering = value(a).total_cmp(&value(b));
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    rows.truncate(limit.clamp(1, 1000));
    rows
}

fn within(value: f64, min_value: Option<f64>, max_value: Option<f64>) -> bool {
    min_value.is_none_or(|min| value >= min) && max_value.is_none_or(|max| value <= max)
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
