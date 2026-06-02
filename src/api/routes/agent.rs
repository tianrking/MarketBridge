use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::api::ApiState;
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};
use crate::data_lake::LakeManifestQuery;

#[derive(Debug, Deserialize, Default)]
pub struct AgentContextQuery {
    symbols: Option<String>,
    exchanges: Option<String>,
    include_storage: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct AgentCapability {
    name: &'static str,
    endpoint: &'static str,
    purpose: &'static str,
}

pub async fn capabilities() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "domain": "agent_capabilities",
        "capabilities": capability_rows(),
        "notes": [
            "Use /v1/agent/context for a compact one-call market context.",
            "Use persist=true on history/klines requests only for data you want to retain locally.",
            "MarketBridge remains data-only: no orders, balances, or trading authority."
        ]
    }))
}

pub async fn context(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<AgentContextQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.as_ref().cloned().map(parse_csv_set_upper);
    let exchanges = q.exchanges.as_ref().cloned().map(parse_csv_set_lower);
    let limit = q.limit.unwrap_or(50).clamp(1, 500);

    let mut quotes = state
        .bus
        .quote_snapshot_all()
        .await
        .into_iter()
        .filter(|quote| {
            quote_matches(
                quote.instrument_ref.symbol.as_deref(),
                &quote.source_ref.source,
                &symbols,
                &exchanges,
            )
        })
        .collect::<Vec<_>>();
    quotes.sort_by(|a, b| {
        a.instrument_ref
            .symbol
            .cmp(&b.instrument_ref.symbol)
            .then(a.source_ref.source.cmp(&b.source_ref.source))
    });
    quotes.truncate(limit);

    let mut funding = state
        .bus
        .funding_snapshot_all()
        .await
        .into_iter()
        .filter(|tick| quote_matches(Some(&tick.symbol), tick.exchange, &symbols, &exchanges))
        .collect::<Vec<_>>();
    funding.truncate(limit);

    let mut open_interest = state
        .bus
        .open_interest_snapshot_all()
        .await
        .into_iter()
        .filter(|tick| quote_matches(Some(&tick.symbol), tick.exchange, &symbols, &exchanges))
        .collect::<Vec<_>>();
    open_interest.truncate(limit);

    let storage = if q.include_storage.unwrap_or(false) {
        state
            .data_lake_store
            .manifest(LakeManifestQuery {
                domain: Some("candles".to_string()),
                exchange: exchanges.as_ref().and_then(|set| {
                    (set.len() == 1)
                        .then(|| set.iter().next().cloned())
                        .flatten()
                }),
                market: None,
                symbol: symbols.as_ref().and_then(|set| {
                    (set.len() == 1)
                        .then(|| set.iter().next().cloned())
                        .flatten()
                }),
                interval: None,
                candle_type: None,
                day: None,
                limit: Some(limit),
            })
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Json(serde_json::json!({
        "version": "v1",
        "domain": "agent_context",
        "agent_mode": {
            "contract": "read_only_market_data",
            "data_boundary": "no_order_execution_no_wallet_no_strategy_claims",
            "recommended_next_calls": [
                "/v1/history/candles?exchange=binance&symbol=BTCUSDT&candle_type=mark&interval=1m&persist=true",
                "/v1/research/symbol-state?symbol=BTCUSDT&exchange=binance",
                "/v1/research/features?symbols=BTCUSDT&benchmark_symbol=ETHUSDT",
                "/v1/storage/manifest?domain=candles&symbol=BTCUSDT"
            ]
        },
        "capabilities": capability_rows(),
        "snapshots": {
            "quotes": quotes,
            "funding": funding,
            "open_interest": open_interest
        },
        "storage_manifest": storage
    }))
}

fn capability_rows() -> Vec<AgentCapability> {
    vec![
        AgentCapability {
            name: "history_candles",
            endpoint: "/v1/history/candles",
            purpose: "Fetch on-demand spot/futures/mark/index/premiumIndex/funding_rate candles and optionally persist them.",
        },
        AgentCapability {
            name: "storage_manifest",
            endpoint: "/v1/storage/manifest",
            purpose: "Inspect local lake coverage, file paths, watermarks, gaps, duplicates, and stale metrics.",
        },
        AgentCapability {
            name: "orderflow_footprint",
            endpoint: "/v1/market/footprint",
            purpose: "Read price-bin footprint, delta, imbalance, stacked imbalance, and raw trade snippets.",
        },
        AgentCapability {
            name: "research_features",
            endpoint: "/v1/research/features",
            purpose: "Read multi-timeframe research features and correlated asset context.",
        },
        AgentCapability {
            name: "strategy_symbol_state",
            endpoint: "/v1/research/symbol-state",
            purpose: "Read real-time short-squeeze and exhaustion-short states with CVD, OFI, OI change, depth pressure, liquidations, and read-only risk context.",
        },
    ]
}

fn quote_matches(
    symbol: Option<&str>,
    exchange: &str,
    symbols: &Option<HashSet<String>>,
    exchanges: &Option<HashSet<String>>,
) -> bool {
    symbols
        .as_ref()
        .is_none_or(|set| symbol.is_some_and(|symbol| set.contains(&symbol.to_ascii_uppercase())))
        && exchanges
            .as_ref()
            .is_none_or(|set| set.contains(&exchange.to_ascii_lowercase()))
}
