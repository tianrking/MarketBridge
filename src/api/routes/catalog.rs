use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::api::ApiState;
use crate::catalog::{domain_catalog, health_status};
use crate::deribit_cache::DeribitOptionFilter;
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::domains::prediction::book::envelope_from_polymarket_book;
use crate::market_discovery::{
    MarketDiscoveryQuery, PerpetualDiscoveryQuery, discover_markets, discover_perpetuals,
    supported_market_exchanges,
};
use crate::source_roadmap;

#[derive(Debug, Serialize)]
struct CatalogHealth {
    source: String,
    domain: &'static str,
    records: usize,
    stale_records: usize,
    last_received_at_ms: Option<u64>,
    status: &'static str,
}

#[derive(Debug, Deserialize, Default)]
pub struct CatalogSearchQuery {
    pub q: Option<String>,
    pub product: Option<String>,
    pub base: Option<String>,
    pub symbol: Option<String>,
    pub quote: Option<String>,
    pub exchange: Option<String>,
    pub exchanges: Option<String>,
    pub market: Option<String>,
    pub active_only: Option<bool>,
    pub include_endpoints: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct CatalogSearchResponse {
    version: &'static str,
    domain: &'static str,
    query: CatalogSearchResolvedQuery,
    summary: CatalogSearchSummary,
    markets: Vec<CatalogSearchMarket>,
    errors: Vec<crate::market_discovery::DiscoveryError>,
}

#[derive(Debug, Serialize)]
struct CatalogSearchResolvedQuery {
    input: Option<String>,
    base: Option<String>,
    symbol: Option<String>,
    quote: Option<String>,
    market: Option<String>,
    active_only: bool,
    match_mode: &'static str,
}

#[derive(Debug, Serialize)]
struct CatalogSearchSummary {
    exchanges_total: usize,
    markets_total: usize,
    spot_total: usize,
    perp_total: usize,
    quotes: Vec<String>,
    exchanges: Vec<String>,
    data_domains: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct CatalogSearchMarket {
    exchange: String,
    market: String,
    symbol: String,
    native_symbol: String,
    base: Option<String>,
    quote: Option<String>,
    active: bool,
    status: Option<String>,
    contract_type: Option<String>,
    settle_asset: Option<String>,
    data_domains: Vec<&'static str>,
    derived_metrics: Vec<&'static str>,
    endpoints: Option<CatalogSearchEndpoints>,
}

#[derive(Debug, Serialize)]
struct CatalogSearchEndpoints {
    rest: Vec<String>,
    websocket: Vec<String>,
}

pub async fn sources(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "sources": state.source_catalog.clone()
    }))
}

pub async fn search(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<CatalogSearchQuery>,
) -> impl IntoResponse {
    let resolved = resolve_search_query(&q);
    let include_endpoints = q.include_endpoints.unwrap_or(true);
    let market_query = MarketDiscoveryQuery {
        exchange: q.exchange.clone(),
        exchanges: q.exchanges.clone(),
        market: q.market.clone(),
        quote: q.quote.clone().or_else(|| resolved.quote.clone()),
        base: resolved.base.clone(),
        active_only: Some(resolved.active_only),
        limit: Some(q.limit.unwrap_or(50_000).clamp(1, 50_000)),
    };
    let (mut markets, errors) = discover_markets(&state.http, &market_query).await;

    if let Some(symbol) = &resolved.symbol {
        let symbol = symbol.to_ascii_uppercase();
        markets.retain(|row| {
            row.symbol.eq_ignore_ascii_case(&symbol)
                || row.native_symbol.eq_ignore_ascii_case(&symbol)
                || resolved.base.as_deref().is_some_and(|base| {
                    row.base.as_deref().is_some_and(|row_base| row_base == base)
                })
        });
    }

    let mut rows = markets
        .into_iter()
        .map(|market| {
            let data_domains = data_domains_for_market(&market.market);
            let derived_metrics = derived_metrics_for_market(&market.market);
            let endpoints = include_endpoints.then(|| endpoints_for_market(&market));
            CatalogSearchMarket {
                exchange: market.exchange,
                market: market.market,
                symbol: market.symbol,
                native_symbol: market.native_symbol,
                base: market.base,
                quote: market.quote,
                active: market.active,
                status: market.status,
                contract_type: market.contract_type,
                settle_asset: market.settle_asset,
                data_domains,
                derived_metrics,
                endpoints,
            }
        })
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        a.exchange
            .cmp(&b.exchange)
            .then(a.market.cmp(&b.market))
            .then(a.symbol.cmp(&b.symbol))
    });

    let summary = catalog_search_summary(&rows);
    Json(CatalogSearchResponse {
        version: "v1",
        domain: "catalog_search",
        query: resolved,
        summary,
        markets: rows,
        errors,
    })
}

pub async fn markets(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketDiscoveryQuery>,
) -> impl IntoResponse {
    let (markets, errors) = discover_markets(&state.http, &q).await;
    Json(serde_json::json!({
        "version": "v1",
        "domain": "catalog_markets",
        "supported_exchanges": supported_market_exchanges(),
        "markets": markets,
        "errors": errors
    }))
}

fn resolve_search_query(q: &CatalogSearchQuery) -> CatalogSearchResolvedQuery {
    let input = q
        .product
        .as_ref()
        .or(q.q.as_ref())
        .or(q.symbol.as_ref())
        .or(q.base.as_ref())
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty());
    let symbol = q.symbol.as_ref().map(|x| normalize_symbol(x));
    let explicit_base = q.base.as_ref().map(|x| normalize_symbol(x));
    let parsed = input.as_deref().map(parse_product_input);
    let base = explicit_base.or_else(|| parsed.as_ref().and_then(|x| x.base.clone()));
    let symbol = symbol.or_else(|| parsed.as_ref().and_then(|x| x.symbol.clone()));
    let quote = q
        .quote
        .as_ref()
        .map(|x| normalize_symbol(x))
        .or_else(|| parsed.as_ref().and_then(|x| x.quote.clone()));
    let match_mode = if q.symbol.is_some() {
        "symbol"
    } else if base.is_some() {
        "asset"
    } else {
        "all"
    };
    CatalogSearchResolvedQuery {
        input,
        base,
        symbol,
        quote,
        market: q.market.as_ref().map(|x| x.trim().to_ascii_lowercase()),
        active_only: q.active_only.unwrap_or(true),
        match_mode,
    }
}

#[derive(Debug, Clone)]
struct ParsedProductInput {
    base: Option<String>,
    symbol: Option<String>,
    quote: Option<String>,
}

fn parse_product_input(raw: &str) -> ParsedProductInput {
    let clean = normalize_symbol(raw);
    if clean.is_empty() {
        return ParsedProductInput {
            base: None,
            symbol: None,
            quote: None,
        };
    }
    for quote in [
        "USDT", "USDC", "FDUSD", "USD", "TRY", "EUR", "BTC", "ETH", "BNB", "BRL", "KRW", "JPY",
        "GBP", "AUD", "CAD",
    ] {
        if let Some(base) = clean.strip_suffix(quote)
            && !base.is_empty()
        {
            return ParsedProductInput {
                base: Some(base.to_string()),
                symbol: Some(clean),
                quote: Some(quote.to_string()),
            };
        }
    }
    ParsedProductInput {
        base: Some(clean),
        symbol: None,
        quote: None,
    }
}

fn normalize_symbol(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('t')
        .replace(['-', '/', '_', ':'], "")
        .to_ascii_uppercase()
}

fn data_domains_for_market(market: &str) -> Vec<&'static str> {
    match market {
        "perp" => vec![
            "market_quote",
            "market_funding",
            "market_open_interest",
            "market_order_book",
            "market_trade",
            "market_liquidation",
            "market_klines",
        ],
        "spot" => vec![
            "market_quote",
            "market_order_book",
            "market_trade",
            "market_klines",
        ],
        _ => vec!["market_quote"],
    }
}

fn derived_metrics_for_market(market: &str) -> Vec<&'static str> {
    match market {
        "perp" => vec![
            "spread",
            "basis",
            "funding_curve",
            "order_flow",
            "footprint",
            "depth_pressure",
            "trade_imbalance",
            "open_interest_change",
        ],
        "spot" => vec![
            "spread",
            "basis",
            "order_flow",
            "footprint",
            "depth_pressure",
            "trade_imbalance",
        ],
        _ => vec!["spread"],
    }
}

fn endpoints_for_market(market: &crate::market_discovery::MarketListing) -> CatalogSearchEndpoints {
    let exchange = &market.exchange;
    let symbol = &market.symbol;
    let market_kind = &market.market;
    let product_type = if market_kind == "perp" {
        "perp"
    } else {
        "spot"
    };
    let mut rest = vec![
        format!(
            "/v1/market/quotes?symbols={symbol}&exchanges={exchange}&product_type={product_type}"
        ),
        format!(
            "/v1/market/order-books?symbols={symbol}&exchanges={exchange}&market={market_kind}"
        ),
        format!("/v1/market/trades?symbols={symbol}&exchanges={exchange}&market={market_kind}"),
        format!(
            "/v1/market/klines?exchange={exchange}&market={market_kind}&symbol={symbol}&interval=1m"
        ),
        format!(
            "/v1/market/order-flow?exchange={exchange}&market={market_kind}&symbol={symbol}&window_ms=60000"
        ),
        format!(
            "/v1/market/footprint?exchange={exchange}&market={market_kind}&symbol={symbol}&interval_ms=60000"
        ),
    ];
    let mut websocket = vec![format!(
        "/v1/stream?domains=market_quote,trade,order_book&symbols={symbol}&exchanges={exchange}&product_type={product_type}"
    )];
    if market_kind == "perp" {
        rest.extend([
            format!("/v1/market/funding?symbols={symbol}&exchanges={exchange}"),
            format!("/v1/market/perpetual-funding?exchange={exchange}&symbols={symbol}"),
            format!("/v1/market/open-interest?symbols={symbol}&exchanges={exchange}"),
            format!("/v1/market/liquidations?symbols={symbol}&exchanges={exchange}"),
        ]);
        websocket.push(format!(
            "/v1/stream?domains=funding,open_interest,liquidation&symbols={symbol}&exchanges={exchange}"
        ));
    }
    CatalogSearchEndpoints { rest, websocket }
}

fn catalog_search_summary(rows: &[CatalogSearchMarket]) -> CatalogSearchSummary {
    let mut exchanges = rows
        .iter()
        .map(|row| row.exchange.clone())
        .collect::<Vec<_>>();
    exchanges.sort();
    exchanges.dedup();
    let mut quotes = rows
        .iter()
        .filter_map(|row| row.quote.clone())
        .collect::<Vec<_>>();
    quotes.sort();
    quotes.dedup();
    let mut domains = rows
        .iter()
        .flat_map(|row| row.data_domains.iter().copied())
        .collect::<Vec<_>>();
    domains.sort();
    domains.dedup();
    CatalogSearchSummary {
        exchanges_total: exchanges.len(),
        markets_total: rows.len(),
        spot_total: rows.iter().filter(|row| row.market == "spot").count(),
        perp_total: rows.iter().filter(|row| row.market == "perp").count(),
        quotes,
        exchanges,
        data_domains: domains,
    }
}

pub async fn perpetuals(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<PerpetualDiscoveryQuery>,
) -> impl IntoResponse {
    let (exchanges, errors) = discover_perpetuals(&state.http, &q).await;
    Json(serde_json::json!({
        "version": "v1",
        "domain": "catalog_perpetuals",
        "supported_exchanges": supported_market_exchanges(),
        "exchanges": exchanges,
        "errors": errors
    }))
}

pub async fn domains() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "domains": domain_catalog()
    }))
}

pub async fn source_roadmap() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "boundary": "data_only_no_execution_no_private_keys",
        "sources": source_roadmap::source_roadmap()
    }))
}

pub async fn health(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mut rows = Vec::new();

    let mut quote_health = HashMap::<String, (usize, usize, Option<u64>)>::new();
    for quote in state.bus.quote_snapshot_all().await {
        let entry = quote_health
            .entry(quote.source_ref.source.clone())
            .or_insert((0, 0, None));
        entry.0 += 1;
        if quote.freshness.stale {
            entry.1 += 1;
        }
        entry.2 = entry.2.max(Some(quote.freshness.ts_received));
    }
    for (source, (records, stale_records, last_received_at_ms)) in quote_health {
        rows.push(CatalogHealth {
            source,
            domain: "market_quote",
            records,
            stale_records,
            last_received_at_ms,
            status: health_status(records, stale_records),
        });
    }

    let option_rows = state
        .deribit_cache
        .filtered(DeribitOptionFilter {
            include_stale: true,
            ..Default::default()
        })
        .await;
    let mut option_health = HashMap::<String, (usize, usize, Option<u64>)>::new();
    for option in option_rows {
        let entry = option_health
            .entry(option.summary.venue.clone())
            .or_insert((0, 0, None));
        entry.0 += 1;
        if option.stale {
            entry.1 += 1;
        }
        entry.2 = entry.2.max(Some(option.received_at_ms));
    }
    for (source, (records, stale_records, last_received_at_ms)) in option_health {
        rows.push(CatalogHealth {
            source,
            domain: "options_chain",
            records,
            stale_records,
            last_received_at_ms,
            status: health_status(records, stale_records),
        });
    }

    let polymarket_rows = state.polymarket_cache.all().await;
    let polymarket_records = polymarket_rows.len();
    let polymarket_stale = polymarket_rows.iter().filter(|row| row.stale).count();
    let polymarket_last = polymarket_rows.iter().map(|row| row.received_at_ms).max();
    rows.push(CatalogHealth {
        source: "polymarket".to_string(),
        domain: "prediction_book",
        records: polymarket_records,
        stale_records: polymarket_stale,
        last_received_at_ms: polymarket_last,
        status: health_status(polymarket_records, polymarket_stale),
    });

    rows.sort_by(|a, b| a.source.cmp(&b.source).then(a.domain.cmp(b.domain)));
    Json(serde_json::json!({
        "version": "v1",
        "health": rows
    }))
}

pub async fn instruments(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mut seen = HashSet::new();
    let mut instruments = Vec::new();

    for quote in state.bus.quote_snapshot_all().await {
        if seen.insert(quote.instrument_ref.instrument_id.clone()) {
            instruments.push(quote.instrument_ref);
        }
    }

    for option in state
        .deribit_cache
        .filtered(DeribitOptionFilter {
            include_stale: true,
            ..Default::default()
        })
        .await
        .into_iter()
        .map(envelope_from_deribit_summary)
    {
        if seen.insert(option.instrument_ref.instrument_id.clone()) {
            instruments.push(option.instrument_ref);
        }
    }

    for book in state
        .polymarket_cache
        .all()
        .await
        .into_iter()
        .map(envelope_from_polymarket_book)
    {
        if seen.insert(book.instrument_ref.instrument_id.clone()) {
            instruments.push(book.instrument_ref);
        }
    }

    instruments.sort_by(|a, b| a.instrument_id.cmp(&b.instrument_id));
    Json(serde_json::json!({
        "version": "v1",
        "instruments": instruments
    }))
}

#[cfg(test)]
mod tests {
    use super::{data_domains_for_market, parse_product_input};

    #[test]
    fn product_input_parses_asset_and_symbol() {
        let parsed = parse_product_input("HOMEUSDT");
        assert_eq!(parsed.base.as_deref(), Some("HOME"));
        assert_eq!(parsed.symbol.as_deref(), Some("HOMEUSDT"));
        assert_eq!(parsed.quote.as_deref(), Some("USDT"));

        let parsed = parse_product_input("home");
        assert_eq!(parsed.base.as_deref(), Some("HOME"));
        assert_eq!(parsed.symbol, None);
    }

    #[test]
    fn perp_capabilities_include_funding_and_oi() {
        let domains = data_domains_for_market("perp");
        assert!(domains.contains(&"market_funding"));
        assert!(domains.contains(&"market_open_interest"));
        assert!(domains.contains(&"market_liquidation"));
    }
}
