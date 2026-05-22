use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::api::ApiState;
use crate::api::utils::{parse_csv_set_upper, parse_csv_vec};
use crate::core::schema::{DataEnvelope, ProductType};
use crate::domains::market::quote::QuotePayload;
use crate::klines::{KlineBar, KlineQuery};
use crate::types::{FundingRateTick, MarketKind, OpenInterestTick, OrderBookTick};

#[derive(Debug, Deserialize, Default)]
pub struct ResearchFeatureQuery {
    exchange: Option<String>,
    market: Option<String>,
    symbols: Option<String>,
    intervals: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    benchmark_symbol: Option<String>,
    correlated_symbols: Option<String>,
    depth_levels: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct CorrelatedAssetFeature {
    symbol: String,
    rolling_return_pct: Option<f64>,
    correlation: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ResearchFeatureRow {
    exchange: String,
    market: String,
    symbol: String,
    interval: String,
    bars: usize,
    first_open_time_ms: u64,
    last_open_time_ms: u64,
    last_close: f64,
    rolling_return_pct: Option<f64>,
    rolling_volume: Option<f64>,
    realized_volatility: Option<f64>,
    z_score: Option<f64>,
    benchmark_symbol: Option<String>,
    benchmark_correlation: Option<f64>,
    correlated_assets: Vec<CorrelatedAssetFeature>,
    funding_rate: Option<f64>,
    open_interest: Option<f64>,
    open_interest_value: Option<f64>,
    basis_bps: Option<f64>,
    basis_regime: String,
    funding_oi_regime: String,
    quote_spread_bps: Option<f64>,
    order_book_depth_notional: Option<f64>,
    liquidity_score: Option<f64>,
    liquidity_regime: String,
    exchange_disagreement_bps: Option<f64>,
}

#[derive(Debug, Serialize)]
struct MarketRegimeSnapshot {
    rows: usize,
    median_realized_volatility: Option<f64>,
    median_basis_bps: Option<f64>,
    median_funding_rate: Option<f64>,
    median_liquidity_score: Option<f64>,
    median_exchange_disagreement_bps: Option<f64>,
    regime: String,
}

#[derive(Clone)]
struct MarketContext {
    quotes: Vec<DataEnvelope<QuotePayload>>,
    funding: HashMap<String, FundingRateTick>,
    open_interest: HashMap<String, OpenInterestTick>,
    order_books: HashMap<String, OrderBookTick>,
    benchmark_returns: HashMap<String, Vec<f64>>,
    correlated_returns: HashMap<String, Vec<CorrelatedAssetReturns>>,
}

#[derive(Clone)]
struct CorrelatedAssetReturns {
    symbol: String,
    returns: Vec<f64>,
    rolling_return_pct: Option<f64>,
}

#[derive(Default)]
struct KlineGroup {
    exchange: String,
    market: String,
    symbol: String,
    interval: String,
    bars: Vec<KlineBar>,
}

pub async fn features(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ResearchFeatureQuery>,
) -> impl IntoResponse {
    let groups = load_groups(&state, &q).await;
    let context = load_context(&state, &q, &groups).await;
    let mut rows = groups
        .into_iter()
        .filter(|group| is_target_group(group, &q))
        .filter_map(|group| feature_row(group, &q, &context))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        a.exchange
            .cmp(&b.exchange)
            .then(a.market.cmp(&b.market))
            .then(a.symbol.cmp(&b.symbol))
            .then(a.interval.cmp(&b.interval))
    });
    rows.truncate(q.limit.unwrap_or(200).clamp(1, 1000));
    Json(serde_json::json!({"version":"v1","domain":"research_features","rows":rows}))
}

pub async fn market_regime(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ResearchFeatureQuery>,
) -> impl IntoResponse {
    let groups = load_groups(&state, &q).await;
    let context = load_context(&state, &q, &groups).await;
    let rows = groups
        .into_iter()
        .filter(|group| is_target_group(group, &q))
        .filter_map(|group| feature_row(group, &q, &context))
        .collect::<Vec<_>>();
    let snapshot = regime_snapshot(&rows);
    Json(serde_json::json!({"version":"v1","domain":"research_market_regime","snapshot":snapshot}))
}

async fn load_groups(state: &ApiState, q: &ResearchFeatureQuery) -> Vec<KlineGroup> {
    let symbols = query_symbol_set(q);
    let intervals = q
        .intervals
        .as_deref()
        .map(parse_csv_vec)
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| vec!["1h".to_string(), "4h".to_string(), "1d".to_string()]);
    let mut grouped = BTreeMap::<String, KlineGroup>::new();

    for interval in intervals {
        let query = KlineQuery {
            exchange: q.exchange.as_ref().map(|x| x.trim().to_ascii_lowercase()),
            market: q.market.as_ref().map(|x| x.trim().to_ascii_lowercase()),
            symbol: None,
            interval: Some(interval),
            start_ms: q.start_ms,
            end_ms: q.end_ms,
            limit: q.limit.unwrap_or(5000).clamp(1, 5000),
        };
        let Ok(rows) = state.kline_store.query(query).await else {
            continue;
        };
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
    }

    grouped
        .into_values()
        .map(|mut group| {
            group.bars.sort_by_key(|bar| bar.open_time_ms);
            group
        })
        .collect()
}

async fn load_context(
    state: &ApiState,
    q: &ResearchFeatureQuery,
    groups: &[KlineGroup],
) -> MarketContext {
    let quotes = state.bus.quote_snapshot_all().await;
    let funding = state
        .bus
        .funding_snapshot_all()
        .await
        .into_iter()
        .map(|tick| (perp_key(tick.exchange, &tick.symbol), tick))
        .collect();
    let open_interest = state
        .bus
        .open_interest_snapshot_all()
        .await
        .into_iter()
        .map(|tick| (perp_key(tick.exchange, &tick.symbol), tick))
        .collect();
    let order_books = state
        .bus
        .order_book_snapshots_matching(|_| true)
        .await
        .into_iter()
        .map(|book| (market_key(book.exchange, book.market, &book.symbol), book))
        .collect();
    let benchmark_returns = q
        .benchmark_symbol
        .as_ref()
        .map(|benchmark| benchmark.to_ascii_uppercase())
        .map(|benchmark| benchmark_return_map(groups, &benchmark))
        .unwrap_or_default();
    let correlated_returns = correlated_return_map(groups, q);

    MarketContext {
        quotes,
        funding,
        open_interest,
        order_books,
        benchmark_returns,
        correlated_returns,
    }
}

fn feature_row(
    group: KlineGroup,
    q: &ResearchFeatureQuery,
    context: &MarketContext,
) -> Option<ResearchFeatureRow> {
    let first = group.bars.first()?;
    let last = group.bars.last()?;
    let returns = log_returns(&group.bars);
    let rolling_return_pct = (first.close > 0.0)
        .then_some((last.close - first.close) / first.close * 100.0)
        .filter(|value| value.is_finite());
    let rolling_volume = sum_volume(&group.bars);
    let realized_volatility = realized_volatility(&returns);
    let z_score = close_z_score(&group.bars);
    let benchmark_symbol = q
        .benchmark_symbol
        .as_ref()
        .map(|symbol| symbol.trim().to_ascii_uppercase())
        .filter(|symbol| !symbol.is_empty());
    let benchmark_correlation = benchmark_symbol
        .as_ref()
        .and_then(|_| context.benchmark_returns.get(&group.interval))
        .and_then(|benchmark| correlation(&returns, benchmark));
    let correlated_assets = context
        .correlated_returns
        .get(&group.interval)
        .map(|items| {
            items
                .iter()
                .filter(|item| !item.symbol.eq_ignore_ascii_case(&group.symbol))
                .map(|item| CorrelatedAssetFeature {
                    symbol: item.symbol.clone(),
                    rolling_return_pct: item.rolling_return_pct,
                    correlation: correlation(&returns, &item.returns),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let funding = context
        .funding
        .get(&perp_key(&group.exchange, &group.symbol));
    let open_interest = context
        .open_interest
        .get(&perp_key(&group.exchange, &group.symbol));
    let basis_bps = basis_bps(&context.quotes, &group.exchange, &group.symbol);
    let quote_spread_bps = quote_spread_bps(
        &context.quotes,
        &group.exchange,
        &group.market,
        &group.symbol,
    );
    let book_stats = context
        .order_books
        .get(&market_key_string(
            &group.exchange,
            &group.market,
            &group.symbol,
        ))
        .and_then(|book| order_book_stats(book, q.depth_levels.unwrap_or(10).clamp(1, 100)));
    let (order_book_depth_notional, liquidity_score) = book_stats.unwrap_or((None, None));
    let exchange_disagreement_bps =
        exchange_disagreement_bps(&context.quotes, &group.market, &group.symbol);

    Some(ResearchFeatureRow {
        exchange: group.exchange,
        market: group.market,
        symbol: group.symbol,
        interval: group.interval,
        bars: group.bars.len(),
        first_open_time_ms: first.open_time_ms,
        last_open_time_ms: last.open_time_ms,
        last_close: last.close,
        rolling_return_pct,
        rolling_volume,
        realized_volatility,
        z_score,
        benchmark_symbol,
        benchmark_correlation,
        correlated_assets,
        funding_rate: funding.map(|tick| tick.funding_rate),
        open_interest: open_interest.map(|tick| tick.open_interest),
        open_interest_value: open_interest.and_then(|tick| tick.open_interest_value),
        basis_bps,
        basis_regime: basis_regime(basis_bps).to_string(),
        funding_oi_regime: funding_oi_regime(funding, open_interest).to_string(),
        quote_spread_bps,
        order_book_depth_notional,
        liquidity_score,
        liquidity_regime: liquidity_regime(liquidity_score, quote_spread_bps).to_string(),
        exchange_disagreement_bps,
    })
}

fn query_symbol_set(q: &ResearchFeatureQuery) -> Option<HashSet<String>> {
    let mut set = q
        .symbols
        .as_ref()
        .cloned()
        .map(parse_csv_set_upper)
        .unwrap_or_default();
    if let Some(benchmark) = q.benchmark_symbol.as_ref() {
        let benchmark = benchmark.trim().to_ascii_uppercase();
        if !benchmark.is_empty() {
            set.insert(benchmark);
        }
    }
    if let Some(correlated) = q.correlated_symbols.as_ref() {
        set.extend(parse_csv_set_upper(correlated.clone()));
    }
    (!set.is_empty()).then_some(set)
}

fn is_target_group(group: &KlineGroup, q: &ResearchFeatureQuery) -> bool {
    q.symbols
        .as_ref()
        .map(|symbols| parse_csv_set_upper(symbols.clone()))
        .is_none_or(|symbols| symbols.contains(&group.symbol.to_ascii_uppercase()))
}

fn benchmark_return_map(groups: &[KlineGroup], benchmark: &str) -> HashMap<String, Vec<f64>> {
    let mut out = HashMap::new();
    for group in groups {
        if group.symbol.eq_ignore_ascii_case(benchmark) {
            out.insert(group.interval.clone(), log_returns(&group.bars));
        }
    }
    out
}

fn correlated_return_map(
    groups: &[KlineGroup],
    q: &ResearchFeatureQuery,
) -> HashMap<String, Vec<CorrelatedAssetReturns>> {
    let Some(symbols) = q
        .correlated_symbols
        .as_ref()
        .cloned()
        .map(parse_csv_set_upper)
    else {
        return HashMap::new();
    };
    let mut out = HashMap::<String, Vec<CorrelatedAssetReturns>>::new();
    for group in groups {
        if !symbols.contains(&group.symbol.to_ascii_uppercase()) {
            continue;
        }
        out.entry(group.interval.clone())
            .or_default()
            .push(CorrelatedAssetReturns {
                symbol: group.symbol.clone(),
                returns: log_returns(&group.bars),
                rolling_return_pct: rolling_return_pct(&group.bars),
            });
    }
    out
}

fn rolling_return_pct(bars: &[KlineBar]) -> Option<f64> {
    let first = bars.first()?;
    let last = bars.last()?;
    (first.close > 0.0)
        .then_some((last.close - first.close) / first.close * 100.0)
        .filter(|value| value.is_finite())
}

fn log_returns(bars: &[KlineBar]) -> Vec<f64> {
    bars.windows(2)
        .filter_map(|window| {
            let prev = window[0].close;
            let next = window[1].close;
            (prev > 0.0 && next > 0.0).then_some((next / prev).ln())
        })
        .filter(|value| value.is_finite())
        .collect()
}

fn sum_volume(bars: &[KlineBar]) -> Option<f64> {
    let mut total = 0.0;
    let mut found = false;
    for bar in bars {
        if let Some(volume) = bar.volume.filter(|value| value.is_finite()) {
            total += volume;
            found = true;
        }
    }
    found.then_some(total)
}

fn realized_volatility(returns: &[f64]) -> Option<f64> {
    if returns.is_empty() {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance =
        returns.iter().map(|ret| (ret - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    Some(variance.sqrt() * (returns.len() as f64).sqrt())
}

fn close_z_score(bars: &[KlineBar]) -> Option<f64> {
    let closes = bars
        .iter()
        .map(|bar| bar.close)
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    let last = *closes.last()?;
    if closes.len() < 2 {
        return None;
    }
    let mean = closes.iter().sum::<f64>() / closes.len() as f64;
    let variance = closes
        .iter()
        .map(|close| (close - mean).powi(2))
        .sum::<f64>()
        / closes.len() as f64;
    let stddev = variance.sqrt();
    (stddev > 0.0).then_some((last - mean) / stddev)
}

fn correlation(left: &[f64], right: &[f64]) -> Option<f64> {
    let len = left.len().min(right.len());
    if len < 2 {
        return None;
    }
    let left = &left[left.len() - len..];
    let right = &right[right.len() - len..];
    let left_mean = left.iter().sum::<f64>() / len as f64;
    let right_mean = right.iter().sum::<f64>() / len as f64;
    let mut cov = 0.0;
    let mut left_var = 0.0;
    let mut right_var = 0.0;
    for (l, r) in left.iter().zip(right.iter()) {
        let ld = l - left_mean;
        let rd = r - right_mean;
        cov += ld * rd;
        left_var += ld * ld;
        right_var += rd * rd;
    }
    (left_var > 0.0 && right_var > 0.0).then_some(cov / (left_var.sqrt() * right_var.sqrt()))
}

fn basis_bps(quotes: &[DataEnvelope<QuotePayload>], exchange: &str, symbol: &str) -> Option<f64> {
    let spot = latest_mid(quotes, exchange, Some("spot"), symbol)?;
    let perp = latest_mid(quotes, exchange, Some("perp"), symbol)?;
    (spot > 0.0).then_some((perp - spot) / spot * 10_000.0)
}

fn quote_spread_bps(
    quotes: &[DataEnvelope<QuotePayload>],
    exchange: &str,
    market: &str,
    symbol: &str,
) -> Option<f64> {
    quotes
        .iter()
        .filter(|quote| {
            quote.source_ref.source.eq_ignore_ascii_case(exchange)
                && quote_product_label(quote.instrument_ref.product_type)
                    .eq_ignore_ascii_case(market)
                && quote
                    .instrument_ref
                    .symbol
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(symbol))
        })
        .max_by_key(|quote| quote.freshness.ts_received)
        .and_then(|quote| {
            let bid = quote.payload.bid;
            let ask = quote.payload.ask;
            let mid = (bid + ask) / 2.0;
            (bid > 0.0 && ask >= bid && mid > 0.0).then_some((ask - bid) / mid * 10_000.0)
        })
}

fn latest_mid(
    quotes: &[DataEnvelope<QuotePayload>],
    exchange: &str,
    market: Option<&str>,
    symbol: &str,
) -> Option<f64> {
    quotes
        .iter()
        .filter(|quote| {
            quote.source_ref.source.eq_ignore_ascii_case(exchange)
                && market.is_none_or(|market| {
                    quote_product_label(quote.instrument_ref.product_type)
                        .eq_ignore_ascii_case(market)
                })
                && quote
                    .instrument_ref
                    .symbol
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(symbol))
        })
        .max_by_key(|quote| quote.freshness.ts_received)
        .and_then(|quote| {
            let mid = (quote.payload.bid + quote.payload.ask) / 2.0;
            mid.is_finite().then_some(mid)
        })
}

fn order_book_stats(
    book: &OrderBookTick,
    depth_levels: usize,
) -> Option<(Option<f64>, Option<f64>)> {
    let best_bid = book.bids.first()?.price;
    let best_ask = book.asks.first()?.price;
    if best_bid <= 0.0 || best_ask < best_bid {
        return None;
    }
    let bid_depth = book
        .bids
        .iter()
        .take(depth_levels)
        .map(|level| level.price * level.qty)
        .filter(|value| value.is_finite())
        .sum::<f64>();
    let ask_depth = book
        .asks
        .iter()
        .take(depth_levels)
        .map(|level| level.price * level.qty)
        .filter(|value| value.is_finite())
        .sum::<f64>();
    let depth = bid_depth + ask_depth;
    let mid = (best_bid + best_ask) / 2.0;
    let spread_bps = (best_ask - best_bid) / mid * 10_000.0;
    Some((Some(depth), Some(depth / (1.0 + spread_bps.max(0.0)))))
}

fn exchange_disagreement_bps(
    quotes: &[DataEnvelope<QuotePayload>],
    market: &str,
    symbol: &str,
) -> Option<f64> {
    let mids = quotes
        .iter()
        .filter(|quote| {
            quote_product_label(quote.instrument_ref.product_type).eq_ignore_ascii_case(market)
                && quote
                    .instrument_ref
                    .symbol
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(symbol))
        })
        .filter_map(|quote| {
            let mid = (quote.payload.bid + quote.payload.ask) / 2.0;
            (mid > 0.0 && mid.is_finite()).then_some(mid)
        })
        .collect::<Vec<_>>();
    if mids.len() < 2 {
        return None;
    }
    let min = mids.iter().copied().fold(f64::INFINITY, f64::min);
    let max = mids.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = mids.iter().sum::<f64>() / mids.len() as f64;
    (mean > 0.0).then_some((max - min) / mean * 10_000.0)
}

fn regime_snapshot(rows: &[ResearchFeatureRow]) -> MarketRegimeSnapshot {
    let median_realized_volatility = median(rows.iter().filter_map(|row| row.realized_volatility));
    let median_basis_bps = median(rows.iter().filter_map(|row| row.basis_bps));
    let median_funding_rate = median(rows.iter().filter_map(|row| row.funding_rate));
    let median_liquidity_score = median(rows.iter().filter_map(|row| row.liquidity_score));
    let median_exchange_disagreement_bps =
        median(rows.iter().filter_map(|row| row.exchange_disagreement_bps));
    let regime = if median_exchange_disagreement_bps.is_some_and(|value| value > 20.0) {
        "fragmented"
    } else if median_realized_volatility.is_some_and(|value| value > 0.08) {
        "high_volatility"
    } else if median_funding_rate.is_some_and(|value| value.abs() > 0.0005) {
        "leveraged"
    } else {
        "normal"
    };
    MarketRegimeSnapshot {
        rows: rows.len(),
        median_realized_volatility,
        median_basis_bps,
        median_funding_rate,
        median_liquidity_score,
        median_exchange_disagreement_bps,
        regime: regime.to_string(),
    }
}

fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut values = values.filter(|value| value.is_finite()).collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

fn basis_regime(basis_bps: Option<f64>) -> &'static str {
    match basis_bps {
        Some(value) if value > 25.0 => "perp_premium",
        Some(value) if value < -25.0 => "perp_discount",
        Some(_) => "flat",
        None => "unknown",
    }
}

fn funding_oi_regime(
    funding: Option<&FundingRateTick>,
    open_interest: Option<&OpenInterestTick>,
) -> &'static str {
    match (funding.map(|tick| tick.funding_rate), open_interest) {
        (Some(rate), Some(_)) if rate > 0.0003 => "positive_funding_with_oi",
        (Some(rate), Some(_)) if rate < -0.0003 => "negative_funding_with_oi",
        (Some(rate), None) if rate > 0.0003 => "positive_funding",
        (Some(rate), None) if rate < -0.0003 => "negative_funding",
        (Some(_), _) => "neutral_funding",
        (None, Some(_)) => "oi_only",
        (None, None) => "unknown",
    }
}

fn liquidity_regime(liquidity_score: Option<f64>, spread_bps: Option<f64>) -> &'static str {
    if spread_bps.is_some_and(|spread| spread > 30.0) {
        "wide_spread"
    } else if liquidity_score.is_some_and(|score| score > 1_000_000.0) {
        "deep"
    } else if liquidity_score.is_some_and(|score| score > 100_000.0) {
        "normal"
    } else if liquidity_score.is_some() {
        "thin"
    } else {
        "unknown"
    }
}

fn perp_key(exchange: &str, symbol: &str) -> String {
    format!(
        "{}:perp:{}",
        exchange.to_ascii_lowercase(),
        symbol.to_ascii_uppercase()
    )
}

fn market_key(exchange: &str, market: MarketKind, symbol: &str) -> String {
    format!(
        "{}:{}:{}",
        exchange.to_ascii_lowercase(),
        market_kind_label(market),
        symbol.to_ascii_uppercase()
    )
}

fn market_key_string(exchange: &str, market: &str, symbol: &str) -> String {
    format!(
        "{}:{}:{}",
        exchange.to_ascii_lowercase(),
        market.to_ascii_lowercase(),
        symbol.to_ascii_uppercase()
    )
}

fn market_kind_label(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "perp",
    }
}

fn quote_product_label(product_type: ProductType) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(open_time_ms: u64, close: f64, volume: f64) -> KlineBar {
        KlineBar {
            exchange: "binance".to_string(),
            market: "spot".to_string(),
            symbol: "BTCUSDT".to_string(),
            interval: "1h".to_string(),
            open_time_ms,
            close_time_ms: open_time_ms + 3_599_999,
            open: close,
            high: close,
            low: close,
            close,
            volume: Some(volume),
            source: "test".to_string(),
            updated_at_ms: open_time_ms,
        }
    }

    #[test]
    fn computes_return_volume_volatility_and_zscore() {
        let bars = vec![bar(1, 100.0, 1.0), bar(2, 110.0, 2.0), bar(3, 121.0, 3.0)];
        let returns = log_returns(&bars);
        assert_eq!(sum_volume(&bars), Some(6.0));
        assert!(realized_volatility(&returns).is_some());
        assert!(close_z_score(&bars).is_some_and(|value| value > 0.0));
    }

    #[test]
    fn computes_perfect_correlation() {
        let left = vec![0.1, 0.2, 0.3];
        let right = vec![1.0, 2.0, 3.0];
        let value = correlation(&left, &right).expect("correlation");
        assert!((value - 1.0).abs() < 1e-12);
    }

    #[test]
    fn classifies_regimes() {
        assert_eq!(basis_regime(Some(30.0)), "perp_premium");
        assert_eq!(liquidity_regime(Some(10.0), Some(40.0)), "wide_spread");
        assert_eq!(median([1.0, 3.0, 2.0].into_iter()), Some(2.0));
    }
}
