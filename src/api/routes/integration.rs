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
pub struct IntegrationContextQuery {
    symbols: Option<String>,
    exchanges: Option<String>,
    include_storage: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct IntegrationCapability {
    name: &'static str,
    endpoint: &'static str,
    purpose: &'static str,
}

pub async fn capabilities() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": "v1",
        "domain": "integration_capabilities",
        "capabilities": capability_rows(),
        "notes": [
            "Use /v1/integration/context for a compact one-call market context.",
            "Use persist=true on history/klines requests only for data you want to retain locally.",
            "MarketBridge provides data and research scenarios: no orders, live balances, or trading authority."
        ]
    }))
}

pub async fn context(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<IntegrationContextQuery>,
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
        "domain": "integration_context",
        "integration_context": {
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

fn capability_rows() -> Vec<IntegrationCapability> {
    vec![
        IntegrationCapability {
            name: "history_candles",
            endpoint: "/v1/history/candles",
            purpose: "Fetch on-demand spot/futures/mark/index/premiumIndex/funding_rate candles and optionally persist them.",
        },
        IntegrationCapability {
            name: "predicted_funding",
            endpoint: "/v1/market/predicted-funding",
            purpose: "Inspect Hyperliquid's named-venue predicted funding estimates without treating them as settled rates.",
        },
        IntegrationCapability {
            name: "stablecoin_liquidity_context",
            endpoint: "/v1/external/stablecoins",
            purpose: "Inspect DefiLlama circulating stablecoin supply and chain distribution as read-only liquidity context.",
        },
        IntegrationCapability {
            name: "defi_yield_context",
            endpoint: "/v1/external/defi-yields",
            purpose: "Inspect DefiLlama pool APY, TVL, base/reward yield and stablecoin flags without deposit or execution paths.",
        },
        IntegrationCapability {
            name: "bitcoin_mempool_context",
            endpoint: "/v1/onchain/mempool",
            purpose: "Inspect keyless Bitcoin mempool size and recommended fee-rate context without broadcasting transactions or inferring direction.",
        },
        IntegrationCapability {
            name: "deribit_volatility_index_history",
            endpoint: "/v1/history/volatility-index",
            purpose: "Fetch bounded public Deribit volatility-index OHLC history for volatility-regime research.",
        },
        IntegrationCapability {
            name: "storage_manifest",
            endpoint: "/v1/storage/manifest",
            purpose: "Inspect local lake coverage, file paths, watermarks, gaps, duplicates, and stale metrics.",
        },
        IntegrationCapability {
            name: "orderflow_footprint",
            endpoint: "/v1/market/footprint",
            purpose: "Read price-bin footprint, delta, imbalance, stacked imbalance, and raw trade snippets.",
        },
        IntegrationCapability {
            name: "research_features",
            endpoint: "/v1/research/features",
            purpose: "Read multi-timeframe research features and correlated asset context.",
        },
        IntegrationCapability {
            name: "candidate_screening",
            endpoint: "/v1/research/scan",
            purpose: "POST explicit evidence candidates for conditional cost ranking; errors and reference-only rows remain visible.",
        },
        IntegrationCapability {
            name: "live_candidate_screening",
            endpoint: "/v1/research/scan-live",
            purpose: "POST configured instrument routes to screen cached books with explicit identity and costs; no automatic asset equivalence.",
        },
        IntegrationCapability {
            name: "paper_fill_scenarios",
            endpoint: "/v1/research/paper",
            purpose: "POST prefunded fill scenarios; inspect balances and unmatched exposure without order execution.",
        },
        IntegrationCapability {
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
