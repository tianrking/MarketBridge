use std::sync::Arc;

use anyhow::{Context, Result, bail};
use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::api::ApiState;
use crate::klines::{KlineBar, interval_to_ms};
use crate::types::now_ms;

#[derive(Debug, Deserialize, Default)]
pub struct HistoryCandlesQuery {
    exchange: String,
    symbol: String,
    interval: Option<String>,
    market: Option<String>,
    candle_type: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
    pages: Option<usize>,
    persist: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryStablecoinQuery {
    chain: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryLiquidationsQuery {
    exchange: String,
    symbol: String,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryOpenInterestQuery {
    exchange: String,
    symbol: String,
    interval: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
    pages: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryTakerVolumeQuery {
    exchange: String,
    symbol: String,
    period: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryAccountRatioQuery {
    exchange: String,
    symbol: String,
    scope: Option<String>,
    period: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryHistoricalVolatilityQuery {
    exchange: String,
    base_coin: Option<String>,
    quote_coin: Option<String>,
    period: Option<u32>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryVolatilityIndexQuery {
    currency: String,
    resolution: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryBasisQuery {
    exchange: String,
    symbol: String,
    contract_type: Option<String>,
    period: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryTradesQuery {
    exchange: String,
    symbol: String,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
    pages: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryMiningQuery {
    window: Option<String>,
    limit: Option<usize>,
}

const BINANCE_AGG_TRADE_WINDOW_MS: u64 = 60 * 60 * 1000;
type TradeWindow = (Option<u64>, Option<u64>);
type TradeWindowPlan = (Vec<TradeWindow>, usize);

struct HistoricalTradesResult {
    rows: Vec<Value>,
    requested_windows: usize,
    completed_windows: usize,
    truncated_windows: usize,
    requested_start_ms: Option<u64>,
    requested_end_ms: Option<u64>,
    covered_start_ms: Option<u64>,
    covered_end_ms: Option<u64>,
    page_limit: usize,
}

struct AccountRatioResult {
    rows: Vec<Value>,
    next_page_cursor: Option<String>,
}

struct VolatilityIndexResult {
    rows: Vec<Value>,
    continuation: Option<u64>,
}

pub async fn trades(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryTradesQuery>,
) -> impl IntoResponse {
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "binance" => fetch_binance_trades(&state.http, &q).await,
        "okx" => fetch_okx_trades(&state.http, &q).await,
        other => Err(anyhow::anyhow!(
            "unsupported historical trades exchange: {other}; public history is currently available for binance and okx"
        )),
    };
    match result {
        Ok(result) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_trade",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "coverage": "bounded_public_trade_history",
            "coverage_detail": {
                "status": result.coverage_status(),
                "requested_windows": result.requested_windows,
                "completed_windows": result.completed_windows,
                "truncated_windows": result.truncated_windows,
                "requested_start_ms": result.requested_start_ms,
                "requested_end_ms": result.requested_end_ms,
                "covered_start_ms": result.covered_start_ms,
                "covered_end_ms": result.covered_end_ms,
                "page_limit": result.page_limit,
                "provider_window_ms": if q.exchange.eq_ignore_ascii_case("binance") {
                    serde_json::json!(BINANCE_AGG_TRADE_WINDOW_MS)
                } else {
                    Value::Null
                },
            },
            "rows": result.rows,
            "limitations": [
                "Binance aggregate trade history is provider-limited to recent data and one-hour time windows",
                "set pages=N to request multiple provider time windows; coverage_detail reports truncation",
                "CVD is a taker-side proxy derived from public trade side",
                "this endpoint does not reconstruct every private or block execution"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_trade",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

impl HistoricalTradesResult {
    fn coverage_status(&self) -> &'static str {
        if self.completed_windows < self.requested_windows {
            "partial_requested_windows"
        } else if self.truncated_windows > 0 {
            "provider_page_may_be_truncated"
        } else {
            "complete_for_requested_windows"
        }
    }
}

pub async fn open_interest(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryOpenInterestQuery>,
) -> impl IntoResponse {
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "binance" => fetch_binance_open_interest(&state.http, &q).await,
        "bybit" => fetch_bybit_open_interest(&state.http, &q).await,
        "okx" => fetch_okx_open_interest(&state.http, &q).await,
        other => Err(anyhow::anyhow!(
            "unsupported historical open-interest exchange: {other}; public history is currently available for binance, bybit and okx"
        )),
    };
    match result {
        Ok(rows) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_open_interest",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "interval": q.interval.clone().unwrap_or_else(|| "5m".to_string()),
            "coverage": "bounded_public_history",
            "coverage_detail": open_interest_coverage_detail(&rows, &q),
            "rows": rows,
            "limitations": [
                "open interest units remain provider-specific and are returned explicitly",
                "history retention and pagination are provider-controlled",
                "coverage_detail describes one bounded provider page; it is not a completeness proof",
                "open interest is aggregate positioning, not long/short direction",
                "OKX contract history is an aggregate base-currency series and reports provider USD units"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_open_interest",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

pub async fn taker_volume(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryTakerVolumeQuery>,
) -> impl IntoResponse {
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "binance" => fetch_binance_taker_volume(&state.http, &q).await,
        other => Err(anyhow::anyhow!(
            "unsupported historical taker-volume exchange: {other}; public history is currently available for binance"
        )),
    };
    match result {
        Ok(rows) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_taker_volume",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "period": q.period.clone().unwrap_or_else(|| "5m".to_string()),
            "coverage": "bounded_public_history",
            "coverage_detail": taker_volume_coverage_detail(&rows, &q),
            "rows": rows,
            "limitations": [
                "Binance exposes provider aggregate taker volumes, not individual fills",
                "only the provider retention window is available and page completeness is not guaranteed",
                "buy/sell imbalance is descriptive and does not identify informed flow or execution quality"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_taker_volume",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

pub async fn account_ratio(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryAccountRatioQuery>,
) -> impl IntoResponse {
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "binance" => fetch_binance_account_ratio(&state.http, &q).await,
        "bybit" => fetch_bybit_account_ratio(&state.http, &q).await,
        other => Err(anyhow::anyhow!(
            "unsupported historical account-ratio exchange: {other}; public history is currently available for binance and bybit"
        )),
    };
    match result {
        Ok(result) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_account_ratio",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "period": q.period.clone().unwrap_or_else(|| "1h".to_string()),
            "scope": q.scope.clone().unwrap_or_else(|| "top_trader".to_string()),
            "coverage": "bounded_public_history",
            "coverage_detail": account_ratio_coverage_detail(&result.rows, &q, result.next_page_cursor.as_deref()),
            "next_page_cursor": result.next_page_cursor,
            "rows": result.rows,
            "limitations": [
                "account ratio semantics are provider-specific: Binance top-trader account share, top-trader position share or global account share versus Bybit holder-count share",
                "provider pagination and retention are bounded; a cursor means the page is incomplete",
                "long/short ratio is descriptive context and does not identify trader intent or execution"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_account_ratio",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

pub async fn historical_volatility(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryHistoricalVolatilityQuery>,
) -> impl IntoResponse {
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "bybit" => fetch_bybit_historical_volatility(&state.http, &q).await,
        other => Err(anyhow::anyhow!(
            "unsupported historical-volatility exchange: {other}; public history is currently available for bybit"
        )),
    };
    match result {
        Ok(rows) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_historical_volatility",
            "exchange": q.exchange,
            "base_coin": q.base_coin,
            "quote_coin": q.quote_coin,
            "period": q.period,
            "coverage": "bounded_public_history",
            "coverage_detail": historical_volatility_coverage_detail(&rows, &q),
            "rows": rows,
            "limitations": [
                "Bybit historical volatility is an option-market provider metric, not a forecast or executable volatility trade",
                "provider retention and the requested time window are bounded",
                "the metric does not expose the option surface, strike selection, hedge, fees or execution"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_historical_volatility",
            "exchange": q.exchange,
            "base_coin": q.base_coin,
            "quote_coin": q.quote_coin,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

pub async fn volatility_index(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryVolatilityIndexQuery>,
) -> impl IntoResponse {
    match fetch_deribit_volatility_index(&state.http, &q).await {
        Ok(result) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_volatility_index",
            "exchange": "deribit",
            "currency": q.currency,
            "resolution": q.resolution.clone().unwrap_or_else(|| "3600".to_string()),
            "coverage": "bounded_public_history",
            "coverage_detail": volatility_index_coverage_detail(&result.rows, &q, result.continuation),
            "rows": result.rows,
            "limitations": [
                "Deribit volatility-index candles are provider observations, not an implied-volatility surface or forecast",
                "provider retention, resolution and continuation are bounded; inspect coverage_detail before treating a page as complete",
                "the endpoint does not model option positions, hedges, fees, PnL or execution"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_volatility_index",
            "exchange": "deribit",
            "currency": q.currency,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

pub async fn basis(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryBasisQuery>,
) -> impl IntoResponse {
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "binance" => fetch_binance_basis(&state.http, &q).await,
        other => Err(anyhow::anyhow!(
            "unsupported historical basis exchange: {other}; public history is currently available for binance"
        )),
    };
    match result {
        Ok(rows) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_basis",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "contract_type": q.contract_type.clone().unwrap_or_else(|| "PERPETUAL".to_string()),
            "period": q.period.clone().unwrap_or_else(|| "1h".to_string()),
            "coverage": "bounded_public_history",
            "coverage_detail": basis_coverage_detail(&rows, &q),
            "rows": rows,
            "limitations": [
                "basis observations are provider snapshots and not simultaneous executable bid/ask legs",
                "provider retention and page limits are bounded; spot/perp fees, borrow, transfer and inventory are absent",
                "basis convergence is a descriptive response test, not an arbitrage or hedge PnL claim"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_basis",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

pub async fn stablecoins(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryStablecoinQuery>,
) -> impl IntoResponse {
    match fetch_stablecoin_history(&state.http, &q).await {
        Ok(rows) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_stablecoin_supply",
            "provider": "defillama",
            "chain": q.chain.clone().unwrap_or_else(|| "all".to_string()),
            "coverage": "bounded_public_history",
            "coverage_detail": stablecoin_coverage_detail(&rows, &q),
            "rows": rows,
            "limitations": [
                "circulating supply is a provider market-cap series, not exchange inventory, bridge flow or deployable liquidity",
                "the public chart endpoint may revise historical values and controls its retention and cadence",
                "the endpoint exposes the provider peggedUSD aggregate; non-USD pegs require a separate explicit query",
                "history is descriptive context and does not imply a price forecast, reserve claim or execution path"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_stablecoin_supply",
            "provider": "defillama",
            "chain": q.chain.clone().unwrap_or_else(|| "all".to_string()),
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

/// Fetch bounded Bitcoin hashrate and difficulty history from mempool.space.
///
/// The endpoint is intentionally provider-shaped: hashrate is a trailing
/// network estimate and difficulty adjustments are sparse retarget events.
/// It is suitable for reproducible context studies such as hash-ribbon
/// response research, not for miner identity, profitability or execution.
pub async fn mining_history(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryMiningQuery>,
) -> impl IntoResponse {
    let window = match mining_history_window(q.window.as_deref()) {
        Ok(window) => window,
        Err(error) => {
            return Json(serde_json::json!({
                "version": "v1",
                "domain": "history_mining",
                "source": "mempool_space",
                "error": error.to_string(),
                "hashrates": [],
                "difficulty_adjustments": []
            }));
        }
    };
    let limit = q.limit.unwrap_or(500).clamp(2, 5_000);
    let url = format!("https://mempool.space/api/v1/mining/hashrate/{window}");
    let result = async {
        let payload = state
            .http
            .get(url)
            .send()
            .await
            .context("failed to fetch mempool.space hashrate history")?
            .error_for_status()
            .context("mempool.space hashrate history returned an error")?
            .json::<Value>()
            .await
            .context("failed to parse mempool.space hashrate history")?;
        normalize_mining_history(&payload, window, limit)
    }
    .await;

    match result {
        Ok((hashrates, difficulty_adjustments, coverage_detail)) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_mining",
            "source": "mempool_space",
            "window": window,
            "coverage": "bounded_public_history",
            "coverage_detail": coverage_detail,
            "hashrates": hashrates,
            "difficulty_adjustments": difficulty_adjustments,
            "limitations": [
                "hashrate is a provider-estimated network series, not miner identity or realized profitability",
                "difficulty adjustments are sparse retarget events and do not replace a miner cash-flow ledger",
                "provider retention, sampling cadence, revisions and timestamp alignment remain explicit",
                "this read-only endpoint does not sign wallets, broadcast transactions or execute trades"
            ]
        })),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_mining",
            "source": "mempool_space",
            "window": window,
            "error": error.to_string(),
            "hashrates": [],
            "difficulty_adjustments": []
        })),
    }
}

fn mining_history_window(raw: Option<&str>) -> Result<&'static str> {
    match raw.unwrap_or("1y").trim().to_ascii_lowercase().as_str() {
        "1d" => Ok("1d"),
        "1w" => Ok("1w"),
        "1m" => Ok("1m"),
        "3m" => Ok("3m"),
        "6m" => Ok("6m"),
        "1y" => Ok("1y"),
        "2y" => Ok("2y"),
        "3y" => Ok("3y"),
        other => bail!("unsupported mempool.space mining window: {other}"),
    }
}

fn normalize_mining_history(
    payload: &Value,
    window: &str,
    limit: usize,
) -> Result<(Vec<Value>, Vec<Value>, Value)> {
    let mut hashrates = payload
        .get("hashrates")
        .and_then(Value::as_array)
        .context("mempool.space hashrate payload is missing hashrates")?
        .iter()
        .filter_map(|row| {
            let timestamp = row
                .get("timestamp")
                .and_then(|value| value_u64(Some(value)))?;
            let avg_hashrate = value_f64(row.get("avgHashrate"))?;
            (avg_hashrate.is_finite() && avg_hashrate > 0.0).then(|| {
                serde_json::json!({
                    "ts_ms": timestamp.checked_mul(1_000),
                    "avg_hashrate_hs": avg_hashrate,
                    "source": "mempool_space"
                })
            })
        })
        .filter(|row| row.get("ts_ms").is_some_and(|value| !value.is_null()))
        .collect::<Vec<_>>();
    hashrates.sort_by_key(|row| row.get("ts_ms").and_then(Value::as_u64).unwrap_or_default());
    if hashrates.len() > limit {
        hashrates.drain(..hashrates.len() - limit);
    }

    let mut difficulty_adjustments = payload
        .get("difficulty")
        .and_then(Value::as_array)
        .context("mempool.space hashrate payload is missing difficulty")?
        .iter()
        .filter_map(|row| {
            let timestamp = row
                .get("time")
                .or_else(|| row.get("timestamp"))
                .and_then(|value| value_u64(Some(value)))?;
            let difficulty = value_f64(row.get("difficulty"))?;
            (difficulty.is_finite() && difficulty > 0.0).then(|| {
                serde_json::json!({
                    "ts_ms": timestamp.checked_mul(1_000),
                    "height": value_u64(row.get("height")),
                    "difficulty": difficulty,
                    "adjustment": value_f64(row.get("adjustment")),
                    "source": "mempool_space"
                })
            })
        })
        .filter(|row| row.get("ts_ms").is_some_and(|value| !value.is_null()))
        .collect::<Vec<_>>();
    difficulty_adjustments
        .sort_by_key(|row| row.get("ts_ms").and_then(Value::as_u64).unwrap_or_default());
    if difficulty_adjustments.len() > limit {
        difficulty_adjustments.drain(..difficulty_adjustments.len() - limit);
    }

    let coverage_detail = serde_json::json!({
        "status": if hashrates.len() >= limit { "locally_limited" } else { "bounded_provider_history" },
        "window": window,
        "returned_hashrate_rows": hashrates.len(),
        "returned_difficulty_rows": difficulty_adjustments.len(),
        "covered_start_ms": hashrates.iter().filter_map(|row| row.get("ts_ms").and_then(Value::as_u64)).min(),
        "covered_end_ms": hashrates.iter().filter_map(|row| row.get("ts_ms").and_then(Value::as_u64)).max(),
        "limit": limit
    });
    Ok((hashrates, difficulty_adjustments, coverage_detail))
}

fn stablecoin_coverage_detail(rows: &[Value], q: &HistoryStablecoinQuery) -> Value {
    let page_limit = q.limit.unwrap_or(5_000).clamp(1, 5_000);
    serde_json::json!({
        "status": if rows.is_empty() { "empty_or_provider_limited" } else if rows.len() >= page_limit { "locally_limited_or_provider_truncated" } else { "bounded_provider_history" },
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "page_limit": page_limit,
    })
}

fn basis_coverage_detail(rows: &[Value], q: &HistoryBasisQuery) -> Value {
    let page_limit = q.limit.unwrap_or(30).clamp(1, 500);
    serde_json::json!({
        "status": if rows.is_empty() { "empty_or_provider_limited" } else if rows.len() >= page_limit { "provider_page_may_be_truncated" } else { "bounded_single_page" },
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "page_limit": page_limit,
    })
}

fn historical_volatility_coverage_detail(
    rows: &[Value],
    q: &HistoryHistoricalVolatilityQuery,
) -> Value {
    serde_json::json!({
        "status": if rows.is_empty() { "empty_or_provider_limited" } else { "bounded_single_page" },
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "period_days": q.period,
    })
}

fn volatility_index_coverage_detail(
    rows: &[Value],
    q: &HistoryVolatilityIndexQuery,
    continuation: Option<u64>,
) -> Value {
    let page_limit = q.limit.unwrap_or(500).clamp(1, 1_000);
    serde_json::json!({
        "status": if continuation.is_some() || rows.len() >= page_limit { "provider_page_may_be_truncated" } else if rows.is_empty() { "empty_or_provider_limited" } else { "bounded_single_page" },
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "page_limit": page_limit,
        "continuation": continuation,
    })
}

fn account_ratio_coverage_detail(
    rows: &[Value],
    q: &HistoryAccountRatioQuery,
    next_page_cursor: Option<&str>,
) -> Value {
    let page_limit = q.limit.unwrap_or(50);
    let status = if next_page_cursor.is_some() || rows.len() >= page_limit {
        "provider_page_may_be_truncated"
    } else {
        "bounded_single_page"
    };
    serde_json::json!({
        "status": status,
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "page_limit": page_limit,
        "has_next_page": next_page_cursor.is_some(),
    })
}

fn taker_volume_coverage_detail(rows: &[Value], q: &HistoryTakerVolumeQuery) -> Value {
    let page_limit = q.limit.unwrap_or(500).min(300);
    serde_json::json!({
        "status": open_interest_coverage_status(rows.len(), page_limit),
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "page_limit": page_limit,
    })
}

fn open_interest_coverage_detail(rows: &[Value], q: &HistoryOpenInterestQuery) -> Value {
    let page_limit = q.limit.unwrap_or(100).clamp(1, 500);
    serde_json::json!({
        "status": open_interest_coverage_status(rows.len(), page_limit),
        "requested_pages": q.pages.unwrap_or(1).clamp(1, 96),
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "page_limit": page_limit,
    })
}

fn open_interest_coverage_status(returned_rows: usize, page_limit: usize) -> &'static str {
    if returned_rows >= page_limit {
        "provider_page_may_be_truncated"
    } else {
        "bounded_single_page"
    }
}

pub async fn liquidations(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryLiquidationsQuery>,
) -> impl IntoResponse {
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "okx" => fetch_okx_liquidations(&state.http, &q).await,
        "coinex" => fetch_coinex_liquidations(&state.http, &q).await,
        other => Err(anyhow::anyhow!(
            "unsupported liquidation history exchange: {other}; public history is currently available for okx and coinex"
        )),
    };
    match result {
        Ok(rows) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_liquidation",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "coverage": "bounded_recent_public_feed",
            "coverage_detail": liquidation_coverage_detail(&rows, &q),
            "rows": rows,
            "limitations": [
                "public history coverage and retention are provider-controlled",
                "coverage_detail describes one bounded provider page; it is not a completeness proof",
                "this endpoint does not reconstruct every venue or every liquidation child fill",
                "OI, CVD, price impact and queue timing must be joined independently"
            ]
        }))
        .into_response(),
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_liquidation",
            "exchange": q.exchange,
            "symbol": q.symbol,
            "error": error.to_string(),
            "rows": []
        }))
        .into_response(),
    }
}

fn liquidation_coverage_detail(rows: &[Value], q: &HistoryLiquidationsQuery) -> Value {
    let limit = q.limit.unwrap_or(100).clamp(1, 100);
    serde_json::json!({
        "status": liquidation_coverage_status(rows.len(), limit),
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).min(),
        "covered_end_ms": rows.iter().filter_map(|row| value_u64(row.get("ts_ms"))).max(),
        "returned_rows": rows.len(),
        "page_limit": limit,
    })
}

fn liquidation_coverage_status(returned_rows: usize, page_limit: usize) -> &'static str {
    if returned_rows >= page_limit {
        "provider_page_may_be_truncated"
    } else {
        "bounded_single_page"
    }
}

pub async fn candles(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryCandlesQuery>,
) -> impl IntoResponse {
    let candle_type = q
        .candle_type
        .as_deref()
        .unwrap_or(q.market.as_deref().unwrap_or("spot"))
        .trim()
        .to_ascii_lowercase();
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "binance" => fetch_binance_history(&state.http, &q, &candle_type).await,
        "okx" => fetch_okx_history(&state.http, &q, &candle_type).await,
        "bybit" => fetch_bybit_history(&state.http, &q, &candle_type).await,
        "hyperliquid" => fetch_hyperliquid_history(&state.http, &q, &candle_type).await,
        "coinbase" => fetch_coinbase_history(&state.http, &q, &candle_type).await,
        other => Err(anyhow::anyhow!("unsupported history exchange: {other}")),
    };

    match result {
        Ok(mut rows) => {
            rows.sort_by_key(|row| row.open_time_ms);
            let funding_schedule = (candle_type == "funding_rate").then(|| funding_schedule(&rows));
            let persist_result = if q.persist.unwrap_or(false) {
                match state
                    .data_lake_store
                    .persist_klines(rows.clone(), candle_type.clone())
                    .await
                {
                    Ok(partitions) => serde_json::json!({"ok": true, "partitions": partitions}),
                    Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
                }
            } else {
                serde_json::json!({"ok": false, "reason": "persist_query_param_not_set"})
            };
            Json(serde_json::json!({
                "version": "v1",
                "domain": "history_candles",
                "exchange": q.exchange,
                "symbol": q.symbol,
                "candle_type": candle_type,
                "persist": persist_result,
                "coverage": "bounded_public_history",
                "coverage_detail": candle_coverage_detail(&rows, &q),
                "funding_schedule": funding_schedule,
                "candles": rows
            }))
        }
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_candles",
            "error": error.to_string(),
            "candles": []
        })),
    }
}

fn candle_coverage_detail(rows: &[KlineBar], q: &HistoryCandlesQuery) -> Value {
    let requested_limit = q.limit.unwrap_or(500);
    let page_limit = if q.exchange.trim().eq_ignore_ascii_case("coinbase") {
        requested_limit.min(300)
    } else {
        requested_limit
    };
    serde_json::json!({
        "status": candle_coverage_status(rows.len(), page_limit),
        "requested_pages": q.pages.unwrap_or(1).clamp(1, 48),
        "requested_start_ms": q.start_ms,
        "requested_end_ms": q.end_ms,
        "covered_start_ms": rows.iter().map(|row| row.open_time_ms).min(),
        "covered_end_ms": rows.iter().map(|row| row.open_time_ms).max(),
        "returned_rows": rows.len(),
        "page_limit": page_limit,
    })
}

fn candle_coverage_status(returned_rows: usize, page_limit: usize) -> &'static str {
    if returned_rows >= page_limit {
        "provider_page_may_be_truncated"
    } else {
        "bounded_single_page"
    }
}

fn funding_schedule(rows: &[KlineBar]) -> Value {
    let mut points = Vec::new();
    let mut observed = std::collections::BTreeSet::new();
    for window in rows.windows(2) {
        let current = &window[0];
        let next = &window[1];
        let Some(interval_ms) = next.open_time_ms.checked_sub(current.open_time_ms) else {
            continue;
        };
        if interval_ms == 0 {
            continue;
        }
        observed.insert(interval_ms);
        points.push(serde_json::json!({
            "funding_time_ms": current.open_time_ms,
            "next_funding_time_ms": next.open_time_ms,
            "interval_ms": interval_ms,
            "applies_to_rate_at_funding_time": true
        }));
    }
    serde_json::json!({
        "version": "adjacent_funding_timestamps/v1",
        "method": "next_observed_funding_timestamp_minus_current",
        "point_in_time": true,
        "points": points,
        "observed_intervals_ms": observed.into_iter().collect::<Vec<_>>()
    })
}

async fn fetch_stablecoin_history(
    http: &reqwest::Client,
    q: &HistoryStablecoinQuery,
) -> Result<Vec<Value>> {
    let chain = q.chain.as_deref().unwrap_or("all").trim();
    validate_stablecoin_chain(chain)?;
    if let (Some(start_ms), Some(end_ms)) = (q.start_ms, q.end_ms)
        && start_ms > end_ms
    {
        bail!("stablecoin history start_ms must not exceed end_ms");
    }
    let payload = http
        .get(format!(
            "https://stablecoins.llama.fi/stablecoincharts/{chain}"
        ))
        .header("User-Agent", "MarketBridge/0.0.6")
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse DefiLlama stablecoin history")?;
    normalize_stablecoin_history(&payload, chain, q)
}

fn validate_stablecoin_chain(chain: &str) -> Result<()> {
    if chain.is_empty()
        || !chain.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        bail!("stablecoin chain must contain only ASCII letters, digits, '-' or '_'");
    }
    Ok(())
}

fn normalize_stablecoin_history(
    payload: &Value,
    chain: &str,
    q: &HistoryStablecoinQuery,
) -> Result<Vec<Value>> {
    let rows = payload
        .as_array()
        .context("DefiLlama stablecoin history payload is not an array")?;
    let page_limit = q.limit.unwrap_or(5_000).clamp(1, 5_000);
    let mut normalized = rows
        .iter()
        .filter_map(|row| {
            let date_secs = value_u64(row.get("date"))?;
            let ts_ms = date_secs.checked_mul(1_000)?;
            if q.start_ms.is_some_and(|start| ts_ms < start)
                || q.end_ms.is_some_and(|end| ts_ms > end)
            {
                return None;
            }
            let circulating_usd = row
                .get("totalCirculatingUSD")
                .and_then(|value| value.get("peggedUSD"))
                .and_then(|value| value_f64(Some(value)));
            let circulating = row
                .get("totalCirculating")
                .and_then(|value| value.get("peggedUSD"))
                .and_then(|value| value_f64(Some(value)));
            if circulating_usd.is_none() && circulating.is_none() {
                return None;
            }
            Some(serde_json::json!({
                "ts_ms": ts_ms,
                "chain": chain,
                "total_circulating_usd": circulating_usd,
                "total_circulating": circulating,
                "source": "defillama_stablecoincharts",
            }))
        })
        .collect::<Vec<_>>();
    normalized.sort_by_key(|row| value_u64(row.get("ts_ms")).unwrap_or_default());
    if normalized.len() > page_limit {
        let drop_count = normalized.len() - page_limit;
        normalized.drain(..drop_count);
    }
    Ok(normalized)
}

async fn fetch_binance_history(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
    candle_type: &str,
) -> Result<Vec<KlineBar>> {
    if candle_type == "funding_rate" {
        return fetch_binance_funding_rate(http, q).await;
    }
    let interval = q.interval.as_deref().unwrap_or("1m");
    let interval_ms = interval_to_ms(interval).context("unsupported Binance candle interval")?;
    let page_limit = q.limit.unwrap_or(500).clamp(1, 1500);
    let limit = page_limit.to_string();
    let pages = q.pages.unwrap_or(1).clamp(1, 48);
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let (url, symbol_key, market, source) = match candle_type {
        "spot" => (
            "https://api.binance.com/api/v3/klines",
            "symbol",
            "spot",
            "binance_spot_klines",
        ),
        "futures" | "perp" => (
            "https://fapi.binance.com/fapi/v1/klines",
            "symbol",
            "perp",
            "binance_futures_klines",
        ),
        "mark" => (
            "https://fapi.binance.com/fapi/v1/markPriceKlines",
            "symbol",
            "perp",
            "binance_mark_price_klines",
        ),
        "index" => (
            "https://fapi.binance.com/fapi/v1/indexPriceKlines",
            "pair",
            "perp",
            "binance_index_price_klines",
        ),
        "premiumindex" | "premium_index" | "premium" => (
            "https://fapi.binance.com/fapi/v1/premiumIndexKlines",
            "symbol",
            "perp",
            "binance_premium_index_klines",
        ),
        other => bail!("unsupported binance candle_type: {other}"),
    };
    let mut rows = Vec::new();
    for (window_start, window_end) in
        history_windows(q.start_ms, q.end_ms, interval_ms, page_limit, pages)
    {
        let start_query = window_start.to_string();
        let end_query = window_end.to_string();
        let request = http.get(url).query(&[
            (symbol_key, symbol.as_str()),
            ("interval", interval),
            ("limit", limit.as_str()),
            ("startTime", start_query.as_str()),
            ("endTime", end_query.as_str()),
        ]);
        let payload = request
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Vec<Value>>>()
            .await
            .context("failed to parse binance historical candles")?;
        let parsed = payload
            .into_iter()
            .map(|row| {
                parse_binance_array_row(
                    "binance",
                    market,
                    &symbol,
                    interval,
                    source,
                    row,
                    matches!(candle_type, "spot" | "futures" | "perp"),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        rows.extend(parsed.into_iter().filter(|row| {
            q.start_ms.is_none_or(|start| row.open_time_ms >= start)
                && q.end_ms.is_none_or(|end| row.open_time_ms <= end)
        }));
    }
    Ok(rows)
}

fn history_windows(
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    interval_ms: u64,
    page_limit: usize,
    pages: usize,
) -> Vec<(u64, u64)> {
    if interval_ms == 0 || page_limit == 0 || pages == 0 {
        return Vec::new();
    }
    let end = end_ms.unwrap_or_else(now_ms);
    let total_bars = (page_limit as u64).saturating_mul(pages as u64);
    let default_start =
        end.saturating_sub(interval_ms.saturating_mul(total_bars.saturating_sub(1)));
    let start = start_ms.unwrap_or(default_start);
    if start > end {
        return Vec::new();
    }
    let window_span = interval_ms.saturating_mul((page_limit.saturating_sub(1)) as u64);
    let mut cursor = start;
    let mut windows = Vec::with_capacity(pages);
    for _ in 0..pages {
        if cursor > end {
            break;
        }
        let window_end = cursor.saturating_add(window_span).min(end);
        windows.push((cursor, window_end));
        if window_end >= end {
            break;
        }
        let next = window_end.saturating_add(interval_ms);
        if next <= cursor {
            break;
        }
        cursor = next;
    }
    windows
}

async fn fetch_binance_funding_rate(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
) -> Result<Vec<KlineBar>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let limit = q.limit.unwrap_or(500).clamp(1, 1000).to_string();
    let mut request = http
        .get("https://fapi.binance.com/fapi/v1/fundingRate")
        .query(&[("symbol", symbol.as_str()), ("limit", limit.as_str())]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await
        .context("failed to parse binance funding history")?;
    let interval = q.interval.as_deref().unwrap_or("8h");
    let interval_ms = interval_to_ms(interval).unwrap_or(28_800_000);
    payload
        .into_iter()
        .map(|row| {
            let open_time_ms = value_u64(row.get("fundingTime")).context("missing fundingTime")?;
            let funding_rate = value_f64(row.get("fundingRate")).context("missing fundingRate")?;
            Ok(KlineBar {
                exchange: "binance".to_string(),
                market: "perp".to_string(),
                symbol: symbol.clone(),
                interval: interval.to_string(),
                open_time_ms,
                close_time_ms: open_time_ms + interval_ms - 1,
                open: funding_rate,
                high: funding_rate,
                low: funding_rate,
                close: funding_rate,
                volume: None,
                source: "binance_funding_rate_history".to_string(),
                updated_at_ms: now_ms(),
            })
        })
        .collect()
}

async fn fetch_coinbase_history(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
    candle_type: &str,
) -> Result<Vec<KlineBar>> {
    if candle_type != "spot" {
        bail!("unsupported coinbase candle_type: {candle_type}; only spot candles are public here");
    }
    let interval = q.interval.as_deref().unwrap_or("1m");
    let granularity = coinbase_granularity(interval)
        .context("unsupported Coinbase interval; use 1m, 5m, 15m, 1h, 6h or 1d")?;
    let product_id = coinbase_product_id(&q.symbol);
    let mut request = http
        .get(format!(
            "https://api.exchange.coinbase.com/products/{product_id}/candles"
        ))
        .header("User-Agent", "MarketBridge/0.0.6")
        .query(&[("granularity", granularity.to_string())]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("start", rfc3339_ms(start_ms)?)]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("end", rfc3339_ms(end_ms)?)]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Vec<Value>>>()
        .await
        .context("failed to parse Coinbase historical candles")?;
    payload
        .into_iter()
        .take(q.limit.unwrap_or(300).clamp(1, 300))
        .map(|row| parse_coinbase_row(&q.symbol, interval, row))
        .collect()
}

fn rfc3339_ms(value: u64) -> Result<String> {
    let millis = i64::try_from(value).context("timestamp exceeds RFC3339 range")?;
    DateTime::<Utc>::from_timestamp_millis(millis)
        .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
        .context("timestamp is outside RFC3339 range")
}

fn coinbase_granularity(interval: &str) -> Option<u64> {
    match interval {
        "1m" => Some(60),
        "5m" => Some(300),
        "15m" => Some(900),
        "1h" => Some(3_600),
        "6h" => Some(21_600),
        "1d" => Some(86_400),
        _ => None,
    }
}

fn coinbase_product_id(symbol: &str) -> String {
    let symbol = symbol.trim().to_ascii_uppercase();
    if symbol.contains('-') {
        return symbol;
    }
    let base = symbol
        .strip_suffix("USDT")
        .or_else(|| symbol.strip_suffix("USDC"))
        .or_else(|| symbol.strip_suffix("USD"))
        .unwrap_or(&symbol);
    format!("{base}-USD")
}

fn parse_coinbase_row(symbol: &str, interval: &str, row: Vec<Value>) -> Result<KlineBar> {
    if row.len() < 6 {
        bail!("short Coinbase candle row");
    }
    let open_time_secs = value_u64(Some(&row[0])).context("missing Coinbase candle time")?;
    let open_time_ms = open_time_secs
        .checked_mul(1_000)
        .context("Coinbase candle time overflow")?;
    let interval_ms = interval_to_ms(interval).context("unsupported interval")?;
    Ok(KlineBar {
        exchange: "coinbase".to_string(),
        market: "spot".to_string(),
        symbol: symbol.trim().to_ascii_uppercase(),
        interval: interval.to_string(),
        open_time_ms,
        close_time_ms: open_time_ms + interval_ms - 1,
        low: value_f64(Some(&row[1])).context("missing Coinbase low")?,
        high: value_f64(Some(&row[2])).context("missing Coinbase high")?,
        open: value_f64(Some(&row[3])).context("missing Coinbase open")?,
        close: value_f64(Some(&row[4])).context("missing Coinbase close")?,
        volume: value_f64(Some(&row[5])),
        source: "coinbase_exchange_candles".to_string(),
        updated_at_ms: now_ms(),
    })
}

async fn fetch_okx_history(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
    candle_type: &str,
) -> Result<Vec<KlineBar>> {
    if candle_type == "funding_rate" {
        return fetch_okx_funding_rate(http, q).await;
    }
    let interval = q.interval.as_deref().unwrap_or("1m");
    let limit = q.limit.unwrap_or(300).clamp(1, 300).to_string();
    let market = q.market.as_deref().unwrap_or("perp");
    let inst_id = okx_inst_id(&q.symbol, market);
    let (url, source, has_volume) = match candle_type {
        "spot" | "futures" | "perp" => (
            "https://www.okx.com/api/v5/market/candles",
            "okx_market_candles",
            true,
        ),
        "mark" => (
            "https://www.okx.com/api/v5/market/mark-price-candles",
            "okx_mark_price_candles",
            false,
        ),
        "index" => (
            "https://www.okx.com/api/v5/market/index-candles",
            "okx_index_candles",
            false,
        ),
        other => bail!("unsupported okx candle_type: {other}"),
    };
    let payload = http
        .get(url)
        .query(&[
            ("instId", inst_id.as_str()),
            ("bar", interval),
            ("limit", limit.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse okx candles")?;
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.into_iter()
        .map(|row| {
            parse_okx_array_row(
                "okx",
                if market == "spot" { "spot" } else { "perp" },
                &q.symbol,
                interval,
                source,
                row,
                has_volume,
            )
        })
        .collect()
}

async fn fetch_okx_funding_rate(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
) -> Result<Vec<KlineBar>> {
    let inst_id = okx_inst_id(&q.symbol, "perp");
    let limit = q.limit.unwrap_or(100).clamp(1, 100).to_string();
    let payload = http
        .get("https://www.okx.com/api/v5/public/funding-rate-history")
        .query(&[("instId", inst_id.as_str()), ("limit", limit.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse okx funding history")?;
    let interval = q.interval.as_deref().unwrap_or("8h");
    let interval_ms = interval_to_ms(interval).unwrap_or(28_800_000);
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.into_iter()
        .map(|row| {
            let open_time_ms = value_u64(row.get("fundingTime")).context("missing fundingTime")?;
            let funding_rate = value_f64(row.get("fundingRate")).context("missing fundingRate")?;
            Ok(KlineBar {
                exchange: "okx".to_string(),
                market: "perp".to_string(),
                symbol: q.symbol.trim().to_ascii_uppercase(),
                interval: interval.to_string(),
                open_time_ms,
                close_time_ms: open_time_ms + interval_ms - 1,
                open: funding_rate,
                high: funding_rate,
                low: funding_rate,
                close: funding_rate,
                volume: None,
                source: "okx_funding_rate_history".to_string(),
                updated_at_ms: now_ms(),
            })
        })
        .collect()
}

async fn fetch_bybit_history(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
    candle_type: &str,
) -> Result<Vec<KlineBar>> {
    if candle_type != "funding_rate" {
        bail!("unsupported bybit candle_type: {candle_type}");
    }
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let limit = q.limit.unwrap_or(200).clamp(1, 200).to_string();
    let mut request = http
        .get("https://api.bybit.com/v5/market/funding/history")
        .query(&[
            ("category", "linear"),
            ("symbol", symbol.as_str()),
            ("limit", limit.as_str()),
        ]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse bybit funding history")?;
    let rows = payload
        .pointer("/result/list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.into_iter()
        .map(|row| {
            let open_time_ms = value_u64(row.get("fundingRateTimestamp"))
                .context("missing fundingRateTimestamp")?;
            let funding_rate = value_f64(row.get("fundingRate")).context("missing fundingRate")?;
            Ok(KlineBar {
                exchange: "bybit".to_string(),
                market: "perp".to_string(),
                symbol: symbol.clone(),
                interval: q.interval.clone().unwrap_or_else(|| "provider".to_string()),
                close_time_ms: open_time_ms,
                open_time_ms,
                open: funding_rate,
                high: funding_rate,
                low: funding_rate,
                close: funding_rate,
                volume: None,
                source: "bybit_funding_rate_history".to_string(),
                updated_at_ms: now_ms(),
            })
        })
        .collect()
}

async fn fetch_hyperliquid_history(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
    candle_type: &str,
) -> Result<Vec<KlineBar>> {
    if candle_type != "funding_rate" {
        bail!("unsupported hyperliquid candle_type: {candle_type}");
    }
    let coin = hyperliquid_coin(&q.symbol);
    let end_ms = q.end_ms.unwrap_or_else(now_ms);
    let start_ms = q
        .start_ms
        .unwrap_or_else(|| end_ms.saturating_sub(30 * 86_400_000));
    if start_ms > end_ms {
        bail!("hyperliquid funding start_ms must not exceed end_ms");
    }
    let payload = http
        .post("https://api.hyperliquid.xyz/info")
        .json(&serde_json::json!({
            "type": "fundingHistory",
            "coin": coin,
            "startTime": start_ms,
            "endTime": end_ms,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await
        .context("failed to parse hyperliquid funding history")?;
    let interval = q.interval.as_deref().unwrap_or("1h").to_string();
    let limit = q.limit.unwrap_or(500).clamp(1, 500);
    let mut rows = payload
        .into_iter()
        .filter_map(|row| {
            let open_time_ms = value_u64(row.get("time"))?;
            let funding_rate = value_f64(row.get("fundingRate"))?;
            Some(KlineBar {
                exchange: "hyperliquid".to_string(),
                market: "perp".to_string(),
                symbol: coin.clone(),
                interval: interval.clone(),
                open_time_ms,
                close_time_ms: open_time_ms,
                open: funding_rate,
                high: funding_rate,
                low: funding_rate,
                close: funding_rate,
                volume: None,
                source: "hyperliquid_funding_history".to_string(),
                updated_at_ms: now_ms(),
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.open_time_ms);
    if rows.len() > limit {
        rows.drain(..rows.len() - limit);
    }
    Ok(rows)
}

fn hyperliquid_coin(symbol: &str) -> String {
    let symbol = symbol.trim().to_ascii_uppercase();
    if symbol.contains(':') {
        return symbol;
    }
    for quote in ["USDT", "USDC", "USD"] {
        if let Some(base) = symbol.strip_suffix(quote)
            && !base.is_empty()
        {
            return base.to_string();
        }
    }
    symbol
}

async fn fetch_okx_liquidations(
    http: &reqwest::Client,
    q: &HistoryLiquidationsQuery,
) -> Result<Vec<Value>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let inst_family = okx_inst_family(&symbol);
    let limit = q.limit.unwrap_or(100).clamp(1, 100).to_string();
    let payload = http
        .get("https://www.okx.com/api/v5/public/liquidation-orders")
        .query(&[
            ("instType", "SWAP"),
            ("instFamily", inst_family.as_str()),
            ("state", "filled"),
            ("limit", limit.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse okx liquidation history")?;
    if payload.get("code").and_then(Value::as_str) != Some("0") {
        bail!(
            "okx liquidation history error: {}",
            payload
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
        );
    }
    let mut rows = Vec::new();
    for group in payload
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let group_symbol = group
            .get("instId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for detail in group
            .get("details")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let ts_ms = value_u64(detail.get("ts").or_else(|| detail.get("time")))
                .context("missing okx liquidation timestamp")?;
            if q.start_ms.is_some_and(|start| ts_ms < start)
                || q.end_ms.is_some_and(|end| ts_ms > end)
            {
                continue;
            }
            let price = value_f64(detail.get("bkPx")).context("missing okx liquidation price")?;
            let qty = value_f64(detail.get("sz")).context("missing okx liquidation size")?;
            rows.push(serde_json::json!({
                "exchange": "okx",
                "symbol": group_symbol,
                "inst_family": group.get("instFamily"),
                "side": detail.get("side").or_else(|| group.get("side")),
                "position_side": detail.get("posSide"),
                "price": price,
                "qty": qty,
                "notional": price * qty,
                "ts_ms": ts_ms,
                "source": "okx_public_liquidation_orders"
            }));
        }
    }
    rows.sort_by_key(|row| row.get("ts_ms").and_then(Value::as_u64).unwrap_or_default());
    Ok(rows)
}

async fn fetch_coinex_liquidations(
    http: &reqwest::Client,
    q: &HistoryLiquidationsQuery,
) -> Result<Vec<Value>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let limit = q.limit.unwrap_or(100).clamp(1, 100).to_string();
    let payload = http
        .get("https://api.coinex.com/v2/futures/liquidation-history")
        .query(&[("market", symbol.as_str()), ("limit", limit.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse coinex liquidation history")?;
    if payload.get("code").and_then(Value::as_i64) != Some(0) {
        bail!(
            "coinex liquidation history error: {}",
            payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
        );
    }
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rows = rows
        .into_iter()
        .filter_map(|row| {
            let ts_ms = value_u64(row.get("created_at"))?;
            if q.start_ms.is_some_and(|start| ts_ms < start)
                || q.end_ms.is_some_and(|end| ts_ms > end)
            {
                return None;
            }
            let raw_side = row.get("side").and_then(Value::as_str).unwrap_or("unknown");
            let side = coinex_liquidation_side(raw_side);
            let price = value_f64(row.get("liq_price"))?;
            let qty = value_f64(row.get("liq_amount"))?;
            Some(serde_json::json!({
                "exchange": "coinex",
                "symbol": symbol,
                "side": side,
                "position_side": raw_side,
                "price": price,
                "qty": qty,
                "notional": price * qty,
                "ts_ms": ts_ms,
                "source": "coinex_public_liquidation_history"
            }))
        })
        .collect::<Vec<_>>();
    Ok(rows)
}

async fn fetch_binance_open_interest(
    http: &reqwest::Client,
    q: &HistoryOpenInterestQuery,
) -> Result<Vec<Value>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let interval = q.interval.as_deref().unwrap_or("5m");
    let interval_ms = interval_to_ms(interval).context("unsupported Binance OI interval")?;
    let page_limit = q.limit.unwrap_or(100).clamp(1, 500);
    let pages = q.pages.unwrap_or(1).clamp(1, 96);
    let limit_query = page_limit.to_string();
    let mut rows = Vec::new();
    for (window_start, window_end) in
        history_windows(q.start_ms, q.end_ms, interval_ms, page_limit, pages)
    {
        let start_query = window_start.to_string();
        let end_query = window_end.to_string();
        let payload = http
            .get("https://fapi.binance.com/futures/data/openInterestHist")
            .query(&[
                ("symbol", symbol.as_str()),
                ("period", interval),
                ("contractType", "PERPETUAL"),
                ("limit", limit_query.as_str()),
                ("startTime", start_query.as_str()),
                ("endTime", end_query.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Value>>()
            .await
            .context("failed to parse binance open-interest history")?;
        for row in payload {
            let ts_ms = value_u64(row.get("timestamp")).context("missing binance OI timestamp")?;
            if q.start_ms.is_some_and(|start| ts_ms < start)
                || q.end_ms.is_some_and(|end| ts_ms > end)
            {
                continue;
            }
            let open_interest =
                value_f64(row.get("sumOpenInterest")).context("missing binance open interest")?;
            rows.push(serde_json::json!({
                "exchange": "binance",
                "symbol": symbol,
                "open_interest": open_interest,
                "open_interest_value": value_f64(row.get("sumOpenInterestValue")),
                "unit": "provider_contracts",
                "ts_ms": ts_ms,
                "source": "binance_open_interest_hist"
            }));
        }
    }
    Ok(rows)
}

async fn fetch_binance_taker_volume(
    http: &reqwest::Client,
    q: &HistoryTakerVolumeQuery,
) -> Result<Vec<Value>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let period = q.period.as_deref().unwrap_or("5m");
    let allowed_period = matches!(
        period,
        "5m" | "15m" | "30m" | "1h" | "2h" | "4h" | "6h" | "12h" | "1d"
    );
    if !allowed_period {
        bail!("unsupported Binance taker-volume period: {period}");
    }
    let limit = q.limit.unwrap_or(500).clamp(1, 500).to_string();
    let mut request = http
        .get("https://fapi.binance.com/futures/data/takerBuySellVol")
        .query(&[
            ("symbol", symbol.as_str()),
            ("contractType", "PERPETUAL"),
            ("period", period),
            ("limit", limit.as_str()),
        ]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await
        .context("failed to parse binance taker buy/sell volume history")?;
    payload
        .into_iter()
        .map(normalize_binance_taker_volume_row)
        .collect()
}

fn normalize_binance_taker_volume_row(row: Value) -> Result<Value> {
    let ts_ms =
        value_u64(row.get("timestamp")).context("missing Binance taker-volume timestamp")?;
    let buy_volume =
        value_f64(row.get("takerBuyVol")).context("missing Binance taker buy volume")?;
    let sell_volume =
        value_f64(row.get("takerSellVol")).context("missing Binance taker sell volume")?;
    let buy_value = value_f64(row.get("takerBuyVolValue"));
    let sell_value = value_f64(row.get("takerSellVolValue"));
    let total_volume = buy_volume + sell_volume;
    let total_value = buy_value.zip(sell_value).map(|(buy, sell)| buy + sell);
    let imbalance = (total_volume > 0.0).then_some((buy_volume - sell_volume) / total_volume);
    Ok(serde_json::json!({
        "exchange": "binance",
        "symbol": row.get("symbol"),
        "contract_type": row.get("contractType"),
        "taker_buy_volume": buy_volume,
        "taker_sell_volume": sell_volume,
        "taker_buy_value": buy_value,
        "taker_sell_value": sell_value,
        "total_volume": total_volume,
        "total_value": total_value,
        "imbalance": imbalance,
        "buy_sell_ratio": (sell_volume > 0.0).then_some(buy_volume / sell_volume),
        "ts_ms": ts_ms,
        "source": "binance_taker_buy_sell_volume"
    }))
}

async fn fetch_bybit_open_interest(
    http: &reqwest::Client,
    q: &HistoryOpenInterestQuery,
) -> Result<Vec<Value>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let interval = bybit_oi_interval(q.interval.as_deref().unwrap_or("5m"))?;
    let limit = q.limit.unwrap_or(100).clamp(1, 200).to_string();
    let mut request = http
        .get("https://api.bybit.com/v5/market/open-interest")
        .query(&[
            ("category", "linear"),
            ("symbol", symbol.as_str()),
            ("intervalTime", interval),
            ("limit", limit.as_str()),
        ]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse bybit open-interest history")?;
    if payload.get("retCode").and_then(Value::as_i64) != Some(0) {
        bail!(
            "bybit open-interest history error: {}",
            payload
                .get("retMsg")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
        );
    }
    let rows = payload
        .pointer("/result/list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.into_iter()
        .map(|row| {
            let ts_ms = value_u64(row.get("timestamp")).context("missing bybit OI timestamp")?;
            let open_interest =
                value_f64(row.get("openInterest")).context("missing bybit open interest")?;
            Ok(serde_json::json!({
                "exchange": "bybit",
                "symbol": symbol,
                "open_interest": open_interest,
                "open_interest_value": null,
                "unit": "base_asset",
                "ts_ms": ts_ms,
                "source": "bybit_open_interest_history"
            }))
        })
        .collect()
}

fn okx_open_interest_period(interval: &str) -> Result<&'static str> {
    match interval.trim().to_ascii_lowercase().as_str() {
        "5m" | "5min" => Ok("5m"),
        "1h" | "1hour" => Ok("1H"),
        "1d" | "1day" => Ok("1D"),
        other => bail!("unsupported OKX open-interest period: {other}; use 5m, 1h or 1d"),
    }
}

fn okx_open_interest_currency(symbol: &str) -> String {
    let normalized = symbol.trim().to_ascii_uppercase();
    if let Some((base, _)) = normalized.split_once('-') {
        return base.to_string();
    }
    ["USDT", "USDC", "USD"]
        .iter()
        .find_map(|quote| normalized.strip_suffix(quote))
        .filter(|base| !base.is_empty())
        .unwrap_or(&normalized)
        .to_string()
}

fn normalize_okx_open_interest_row(symbol: &str, row: &Value) -> Result<Value> {
    let values = row
        .as_array()
        .context("missing OKX open-interest history row")?;
    let ts_ms = value_u64(values.first()).context("invalid OKX OI timestamp")?;
    let open_interest = value_f64(values.get(1)).context("invalid OKX OI value")?;
    let volume = value_f64(values.get(2));
    Ok(serde_json::json!({
        "exchange": "okx",
        "symbol": symbol,
        "open_interest": open_interest,
        "open_interest_value": open_interest,
        "volume": volume,
        "unit": "provider_usd",
        "ts_ms": ts_ms,
        "source": "okx_contracts_open_interest_volume"
    }))
}

async fn fetch_okx_open_interest(
    http: &reqwest::Client,
    q: &HistoryOpenInterestQuery,
) -> Result<Vec<Value>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let currency = okx_open_interest_currency(&symbol);
    if currency.is_empty() {
        bail!("OKX open-interest history requires a non-empty base currency")
    }
    let period = okx_open_interest_period(q.interval.as_deref().unwrap_or("5m"))?;
    let mut request = http
        .get("https://www.okx.com/api/v5/rubik/stat/contracts/open-interest-volume")
        .query(&[("ccy", currency.as_str()), ("period", period)]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("begin", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("end", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse OKX open-interest history")?;
    if payload.get("code").and_then(Value::as_str) != Some("0") {
        bail!(
            "OKX open-interest history error: {}",
            payload
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
        );
    }
    let mut rows = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| normalize_okx_open_interest_row(&symbol, &row))
        .collect::<Result<Vec<_>>>()?;
    rows.sort_by_key(|row| value_u64(row.get("ts_ms")).unwrap_or_default());
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    if rows.len() > limit {
        let start = rows.len() - limit;
        rows = rows.split_off(start);
    }
    Ok(rows)
}

async fn fetch_bybit_account_ratio(
    http: &reqwest::Client,
    q: &HistoryAccountRatioQuery,
) -> Result<AccountRatioResult> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let period = match q.period.as_deref().unwrap_or("1h") {
        "5m" | "5min" => "5min",
        "15m" | "15min" => "15min",
        "30m" | "30min" => "30min",
        "1h" => "1h",
        "4h" => "4h",
        "1d" => "1d",
        other => bail!("unsupported Bybit account-ratio period: {other}"),
    };
    let limit = q.limit.unwrap_or(50).clamp(1, 500).to_string();
    let mut request = http
        .get("https://api.bybit.com/v5/market/account-ratio")
        .query(&[
            ("category", "linear"),
            ("symbol", symbol.as_str()),
            ("period", period),
            ("limit", limit.as_str()),
        ]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse bybit account-ratio history")?;
    if payload.get("retCode").and_then(Value::as_i64) != Some(0) {
        bail!(
            "bybit account-ratio error: {}",
            payload
                .get("retMsg")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
        );
    }
    let result = payload.get("result").cloned().unwrap_or(Value::Null);
    let next_page_cursor = result
        .get("nextPageCursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string);
    let rows = result
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(normalize_bybit_account_ratio_row)
        .collect::<Result<Vec<_>>>()?;
    Ok(AccountRatioResult {
        rows,
        next_page_cursor,
    })
}

async fn fetch_binance_account_ratio(
    http: &reqwest::Client,
    q: &HistoryAccountRatioQuery,
) -> Result<AccountRatioResult> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let scope = q
        .scope
        .as_deref()
        .unwrap_or("top_trader")
        .trim()
        .to_ascii_lowercase();
    let (endpoint, source, semantics) = match scope.as_str() {
        "top_trader" => (
            "https://fapi.binance.com/futures/data/topLongShortAccountRatio",
            "binance_top_trader_account_ratio",
            "top_trader_account_share",
        ),
        "global" => (
            "https://fapi.binance.com/futures/data/globalLongShortAccountRatio",
            "binance_global_account_ratio",
            "global_account_share",
        ),
        "top_trader_position" => (
            "https://fapi.binance.com/futures/data/topLongShortPositionRatio",
            "binance_top_trader_position_ratio",
            "top_trader_position_share",
        ),
        other => {
            bail!(
                "unsupported Binance account-ratio scope: {other}; use top_trader, top_trader_position or global"
            )
        }
    };
    let period = match q.period.as_deref().unwrap_or("1h") {
        "5m" | "15m" | "30m" | "1h" | "2h" | "4h" | "6h" | "12h" | "1d" => {
            q.period.as_deref().unwrap_or("1h")
        }
        other => bail!("unsupported Binance account-ratio period: {other}"),
    };
    let limit = q.limit.unwrap_or(30).clamp(1, 500).to_string();
    let mut request = http.get(endpoint).query(&[
        ("symbol", symbol.as_str()),
        ("period", period),
        ("limit", limit.as_str()),
    ]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await
        .context("failed to parse binance top account-ratio history")?;
    let rows = payload
        .into_iter()
        .map(|row| normalize_binance_account_ratio_row(row, source, semantics))
        .collect::<Result<Vec<_>>>()?;
    Ok(AccountRatioResult {
        rows,
        next_page_cursor: None,
    })
}

fn normalize_binance_account_ratio_row(row: Value, source: &str, semantics: &str) -> Result<Value> {
    let ts_ms =
        value_u64(row.get("timestamp")).context("missing Binance account-ratio timestamp")?;
    let buy_ratio = value_f64(row.get("longAccount").or_else(|| row.get("longPosition")))
        .context("missing Binance long-account or long-position share")?;
    let sell_ratio = value_f64(row.get("shortAccount").or_else(|| row.get("shortPosition")))
        .context("missing Binance short-account or short-position share")?;
    let long_short_ratio = value_f64(row.get("longShortRatio"))
        .or_else(|| (sell_ratio > 0.0).then_some(buy_ratio / sell_ratio));
    Ok(serde_json::json!({
        "exchange": "binance",
        "symbol": row.get("symbol").or_else(|| row.get("pair")),
        "buy_ratio": buy_ratio,
        "sell_ratio": sell_ratio,
        "imbalance": buy_ratio - sell_ratio,
        "long_short_ratio": long_short_ratio,
        "ts_ms": ts_ms,
        "source": source,
        "semantics": semantics
    }))
}

async fn fetch_bybit_historical_volatility(
    http: &reqwest::Client,
    q: &HistoryHistoricalVolatilityQuery,
) -> Result<Vec<Value>> {
    let base_coin = q
        .base_coin
        .as_deref()
        .unwrap_or("BTC")
        .trim()
        .to_ascii_uppercase();
    let period = q.period.unwrap_or(7);
    if !matches!(period, 7 | 14 | 21 | 30 | 60 | 90 | 180 | 270) {
        bail!("unsupported Bybit historical-volatility period: {period}");
    }
    if q.start_ms.is_some() != q.end_ms.is_some() {
        bail!("Bybit historical-volatility start_ms and end_ms must be provided together");
    }
    if let (Some(start_ms), Some(end_ms)) = (q.start_ms, q.end_ms)
        && end_ms < start_ms
    {
        bail!("Bybit historical-volatility end_ms must be >= start_ms");
    }
    let mut request = http
        .get("https://api.bybit.com/v5/market/historical-volatility")
        .query(&[
            ("category", "option"),
            ("baseCoin", base_coin.as_str()),
            ("period", period.to_string().as_str()),
        ]);
    if let Some(quote_coin) = q.quote_coin.as_deref() {
        let quote_coin = quote_coin.trim().to_ascii_uppercase();
        if !matches!(quote_coin.as_str(), "USD" | "USDT") {
            bail!("unsupported Bybit historical-volatility quote_coin: {quote_coin}");
        }
        request = request.query(&[("quoteCoin", quote_coin.as_str())]);
    }
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse bybit historical volatility")?;
    if payload.get("retCode").and_then(Value::as_i64) != Some(0) {
        bail!(
            "bybit historical-volatility error: {}",
            payload
                .get("retMsg")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
        );
    }
    let rows = payload
        .pointer("/result/list")
        .and_then(Value::as_array)
        .or_else(|| payload.get("result").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default();
    let quote_coin = q
        .quote_coin
        .as_deref()
        .unwrap_or("USD")
        .to_ascii_uppercase();
    rows.into_iter()
        .map(|row| normalize_bybit_historical_volatility_row(row, &base_coin, &quote_coin, period))
        .collect()
}

async fn fetch_deribit_volatility_index(
    http: &reqwest::Client,
    q: &HistoryVolatilityIndexQuery,
) -> Result<VolatilityIndexResult> {
    let currency = q.currency.trim().to_ascii_uppercase();
    if !matches!(currency.as_str(), "BTC" | "ETH" | "USDC" | "USDT" | "EURR") {
        bail!("unsupported Deribit volatility-index currency: {currency}");
    }
    let resolution = q.resolution.as_deref().unwrap_or("3600").trim().to_string();
    if !matches!(resolution.as_str(), "1" | "60" | "3600" | "43200" | "1D") {
        bail!("unsupported Deribit volatility-index resolution: {resolution}");
    }
    let end_ms = q.end_ms.unwrap_or_else(now_ms);
    let start_ms = q
        .start_ms
        .unwrap_or_else(|| end_ms.saturating_sub(30 * 86_400_000));
    if end_ms < start_ms {
        bail!("Deribit volatility-index end_ms must be >= start_ms");
    }
    let payload = http
        .post("https://www.deribit.com/api/v2/public/get_volatility_index_data")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "public/get_volatility_index_data",
            "params": {
                "currency": currency,
                "start_timestamp": start_ms,
                "end_timestamp": end_ms,
                "resolution": resolution,
            }
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse Deribit volatility-index response")?;
    if let Some(error) = payload.get("error") {
        bail!(
            "Deribit volatility-index error: {}",
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error")
        );
    }
    let raw_rows = payload
        .pointer("/result/data")
        .and_then(Value::as_array)
        .context("Deribit volatility-index response missing result.data")?;
    let mut rows = raw_rows
        .iter()
        .map(|row| normalize_deribit_volatility_index_row(row, &currency, &resolution))
        .collect::<Result<Vec<_>>>()?;
    rows.sort_by_key(|row| value_u64(row.get("ts_ms")).unwrap_or_default());
    let limit = q.limit.unwrap_or(500).clamp(1, 1_000);
    if rows.len() > limit {
        rows.drain(..rows.len() - limit);
    }
    Ok(VolatilityIndexResult {
        rows,
        continuation: payload
            .pointer("/result/continuation")
            .and_then(|value| value_u64(Some(value))),
    })
}

fn normalize_deribit_volatility_index_row(
    row: &Value,
    currency: &str,
    resolution: &str,
) -> Result<Value> {
    let values = row
        .as_array()
        .context("Deribit volatility-index candle must be an array")?;
    if values.len() < 5 {
        bail!("Deribit volatility-index candle must contain timestamp and OHLC values");
    }
    let ts_ms = value_u64(values.first()).context("missing Deribit volatility-index timestamp")?;
    let open = value_f64(values.get(1)).context("missing Deribit volatility-index open")?;
    let high = value_f64(values.get(2)).context("missing Deribit volatility-index high")?;
    let low = value_f64(values.get(3)).context("missing Deribit volatility-index low")?;
    let close = value_f64(values.get(4)).context("missing Deribit volatility-index close")?;
    if [open, high, low, close]
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        bail!("Deribit volatility-index OHLC values must be finite and non-negative");
    }
    Ok(serde_json::json!({
        "exchange": "deribit",
        "currency": currency,
        "resolution": resolution,
        "ts_ms": ts_ms,
        "open": open,
        "high": high,
        "low": low,
        "close": close,
        "source": "deribit_volatility_index"
    }))
}

async fn fetch_binance_basis(http: &reqwest::Client, q: &HistoryBasisQuery) -> Result<Vec<Value>> {
    let pair = q.symbol.trim().to_ascii_uppercase();
    let contract_type = q
        .contract_type
        .as_deref()
        .unwrap_or("PERPETUAL")
        .trim()
        .to_ascii_uppercase();
    if !matches!(
        contract_type.as_str(),
        "PERPETUAL" | "CURRENT_QUARTER" | "NEXT_QUARTER"
    ) {
        bail!("unsupported Binance basis contract_type: {contract_type}");
    }
    let period = match q.period.as_deref().unwrap_or("1h") {
        "5m" | "15m" | "30m" | "1h" | "2h" | "4h" | "6h" | "12h" | "1d" => {
            q.period.as_deref().unwrap_or("1h")
        }
        other => bail!("unsupported Binance basis period: {other}"),
    };
    let limit = q.limit.unwrap_or(30).clamp(1, 500).to_string();
    let mut request = http
        .get("https://fapi.binance.com/futures/data/basis")
        .query(&[
            ("pair", pair.as_str()),
            ("contractType", contract_type.as_str()),
            ("period", period),
            ("limit", limit.as_str()),
        ]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await
        .context("failed to parse binance historical basis")?;
    payload
        .into_iter()
        .map(normalize_binance_basis_row)
        .collect()
}

fn normalize_binance_basis_row(row: Value) -> Result<Value> {
    let ts_ms = value_u64(row.get("timestamp")).context("missing Binance basis timestamp")?;
    let basis = value_f64(row.get("basis")).context("missing Binance basis")?;
    let basis_rate = value_f64(row.get("basisRate")).context("missing Binance basisRate")?;
    let annualized_basis_rate = value_f64(row.get("annualizedBasisRate"));
    Ok(serde_json::json!({
        "exchange": "binance",
        "symbol": row.get("pair").or_else(|| row.get("symbol")),
        "contract_type": row.get("contractType"),
        "index_price": value_f64(row.get("indexPrice")),
        "futures_price": value_f64(row.get("futuresPrice")),
        "basis": basis,
        "basis_rate": basis_rate,
        "basis_rate_bps": basis_rate * 10_000.0,
        "annualized_basis_rate": annualized_basis_rate,
        "annualized_basis_rate_bps": annualized_basis_rate.map(|value| value * 10_000.0),
        "ts_ms": ts_ms,
        "source": "binance_basis_history"
    }))
}

fn normalize_bybit_historical_volatility_row(
    row: Value,
    base_coin: &str,
    quote_coin: &str,
    default_period: u32,
) -> Result<Value> {
    let ts_ms = value_u64(row.get("time")).context("missing Bybit volatility time")?;
    let volatility = value_f64(row.get("value")).context("missing Bybit volatility value")?;
    Ok(serde_json::json!({
        "exchange": "bybit",
        "base_coin": base_coin,
        "quote_coin": quote_coin,
        "period_days": value_u64(row.get("period")).unwrap_or(default_period as u64),
        "volatility": volatility,
        "ts_ms": ts_ms,
        "source": "bybit_historical_volatility"
    }))
}

fn normalize_bybit_account_ratio_row(row: Value) -> Result<Value> {
    let ts_ms = value_u64(row.get("timestamp")).context("missing Bybit account-ratio timestamp")?;
    let buy_ratio = value_f64(row.get("buyRatio")).context("missing Bybit buy ratio")?;
    let sell_ratio = value_f64(row.get("sellRatio")).context("missing Bybit sell ratio")?;
    let total = buy_ratio + sell_ratio;
    Ok(serde_json::json!({
        "exchange": "bybit",
        "symbol": row.get("symbol"),
        "buy_ratio": buy_ratio,
        "sell_ratio": sell_ratio,
        "imbalance": (total > 0.0).then_some((buy_ratio - sell_ratio) / total),
        "long_short_ratio": (sell_ratio > 0.0).then_some(buy_ratio / sell_ratio),
        "ts_ms": ts_ms,
        "source": "bybit_account_ratio"
    }))
}

async fn fetch_binance_trades(
    http: &reqwest::Client,
    q: &HistoryTradesQuery,
) -> Result<HistoricalTradesResult> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let limit = q.limit.unwrap_or(500).clamp(1, 1000).to_string();
    let page_limit = limit.parse::<usize>().unwrap_or(500);
    let pages = q.pages.unwrap_or(1).clamp(1, 48);
    let (windows, requested_windows) = binance_trade_windows(q.start_ms, q.end_ms, pages);
    let mut rows = Vec::new();
    let mut completed_windows = 0;
    let mut truncated_windows = 0;
    for (start_ms, end_ms) in windows {
        let mut request = http
            .get("https://fapi.binance.com/fapi/v1/aggTrades")
            .query(&[("symbol", symbol.as_str()), ("limit", limit.as_str())]);
        if let Some(start_ms) = start_ms {
            request = request.query(&[("startTime", start_ms.to_string())]);
        }
        if let Some(end_ms) = end_ms {
            request = request.query(&[("endTime", end_ms.to_string())]);
        }
        let payload = request
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Value>>()
            .await
            .context("failed to parse binance aggregate trades")?;
        if payload.len() >= page_limit {
            truncated_windows += 1;
        }
        completed_windows += 1;
        rows.extend(payload.into_iter().filter_map(|row| {
            let ts_ms = value_u64(row.get("T"))?;
            if q.start_ms.is_some_and(|start| ts_ms < start)
                || q.end_ms.is_some_and(|end| ts_ms > end)
            {
                return None;
            }
            let price = value_f64(row.get("p"))?;
            let qty = value_f64(row.get("q"))?;
            let is_buyer_maker = row.get("m").and_then(Value::as_bool).unwrap_or(false);
            Some(serde_json::json!({
                "exchange": "binance",
                "symbol": symbol,
                "trade_id": row.get("a"),
                "side": if is_buyer_maker { "sell" } else { "buy" },
                "price": price,
                "qty": qty,
                "notional": price * qty,
                "ts_ms": ts_ms,
                "source": "binance_futures_agg_trades"
            }))
        }));
    }
    rows.sort_by_key(|row| value_u64(row.get("ts_ms")).unwrap_or_default());
    let covered_start_ms = rows
        .iter()
        .filter_map(|row| value_u64(row.get("ts_ms")))
        .min();
    let covered_end_ms = rows
        .iter()
        .filter_map(|row| value_u64(row.get("ts_ms")))
        .max();
    Ok(HistoricalTradesResult {
        rows,
        requested_windows,
        completed_windows,
        truncated_windows,
        requested_start_ms: q.start_ms,
        requested_end_ms: q.end_ms,
        covered_start_ms,
        covered_end_ms,
        page_limit,
    })
}

async fn fetch_okx_trades(
    http: &reqwest::Client,
    q: &HistoryTradesQuery,
) -> Result<HistoricalTradesResult> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let inst_id = okx_inst_id(&symbol, "perp");
    let page_limit = q.limit.unwrap_or(100).clamp(1, 100);
    let pages = q.pages.unwrap_or(1).clamp(1, 20);
    let mut rows = Vec::new();
    let mut cursor_end = q.end_ms;
    let mut completed_windows = 0;
    let mut truncated_windows = 0;
    for _ in 0..pages {
        let mut request = http
            .get("https://www.okx.com/api/v5/market/history-trades")
            .query(&[
                ("instId", inst_id.as_str()),
                ("type", "2"),
                ("limit", page_limit.to_string().as_str()),
            ]);
        if let Some(end_ms) = cursor_end {
            request = request.query(&[("after", end_ms.to_string())]);
        }
        let payload = request
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await
            .context("failed to parse okx history trades")?;
        if payload.get("code").and_then(Value::as_str) != Some("0") {
            bail!(
                "okx history trades error: {}",
                payload
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown provider error")
            );
        }
        let page = payload
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let page_len = page.len();
        if page_len >= page_limit {
            truncated_windows += 1;
        }
        completed_windows += 1;
        let mut oldest_ts = None;
        for row in page {
            let Some(ts_ms) = value_u64(row.get("ts")) else {
                continue;
            };
            oldest_ts = Some(oldest_ts.map_or(ts_ms, |oldest: u64| oldest.min(ts_ms)));
            if q.start_ms.is_some_and(|start| ts_ms < start)
                || q.end_ms.is_some_and(|end| ts_ms > end)
            {
                continue;
            }
            let Some(price) = value_f64(row.get("px")) else {
                continue;
            };
            let Some(qty) = value_f64(row.get("sz")) else {
                continue;
            };
            let side = row.get("side").and_then(Value::as_str).unwrap_or("unknown");
            rows.push(serde_json::json!({
                "exchange": "okx",
                "symbol": symbol,
                "trade_id": row.get("tradeId"),
                "side": side,
                "price": price,
                "qty": qty,
                "notional": price * qty,
                "ts_ms": ts_ms,
                "source": "okx_history_trades"
            }));
        }
        if page_len < page_limit
            || q.start_ms
                .is_some_and(|start| oldest_ts.is_some_and(|oldest| oldest <= start))
        {
            break;
        }
        cursor_end = oldest_ts.map(|oldest| oldest.saturating_sub(1));
        if cursor_end.is_none() {
            break;
        }
    }
    rows.sort_by_key(|row| value_u64(row.get("ts_ms")).unwrap_or_default());
    let covered_start_ms = rows
        .iter()
        .filter_map(|row| value_u64(row.get("ts_ms")))
        .min();
    let covered_end_ms = rows
        .iter()
        .filter_map(|row| value_u64(row.get("ts_ms")))
        .max();
    Ok(HistoricalTradesResult {
        rows,
        requested_windows: pages,
        completed_windows,
        truncated_windows,
        requested_start_ms: q.start_ms,
        requested_end_ms: q.end_ms,
        covered_start_ms,
        covered_end_ms,
        page_limit,
    })
}

fn binance_trade_windows(
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    pages: usize,
) -> TradeWindowPlan {
    if start_ms.is_none() && end_ms.is_none() && pages == 1 {
        return (vec![(None, None)], 1);
    }
    let end = end_ms.unwrap_or_else(now_ms);
    let start = start_ms.unwrap_or_else(|| {
        end.saturating_sub(
            BINANCE_AGG_TRADE_WINDOW_MS
                .saturating_mul(pages as u64)
                .saturating_sub(1),
        )
    });
    let requested_windows = end
        .saturating_sub(start)
        .saturating_add(1)
        .div_ceil(BINANCE_AGG_TRADE_WINDOW_MS) as usize;
    let mut windows = Vec::new();
    let mut cursor = start;
    while cursor <= end && windows.len() < pages {
        let window_end = cursor
            .saturating_add(BINANCE_AGG_TRADE_WINDOW_MS - 1)
            .min(end);
        windows.push((Some(cursor), Some(window_end)));
        if window_end == u64::MAX {
            break;
        }
        cursor = window_end + 1;
    }
    (windows, requested_windows)
}

fn bybit_oi_interval(value: &str) -> Result<&'static str> {
    match value {
        "5m" | "5min" => Ok("5min"),
        "15m" | "15min" => Ok("15min"),
        "30m" | "30min" => Ok("30min"),
        "1h" => Ok("1h"),
        "4h" => Ok("4h"),
        "1d" | "1day" => Ok("1d"),
        other => bail!("unsupported bybit open-interest interval: {other}"),
    }
}

fn parse_binance_array_row(
    exchange: &str,
    market: &str,
    symbol: &str,
    interval: &str,
    source: &str,
    row: Vec<Value>,
    has_volume: bool,
) -> Result<KlineBar> {
    if row.len() < 6 {
        bail!("short binance candle row");
    }
    let open_time_ms = value_u64(row.first()).context("missing open time")?;
    let interval_ms = interval_to_ms(interval).context("unsupported interval")?;
    let close_time_ms = if has_volume {
        row.get(6)
            .and_then(|value| value_u64(Some(value)))
            .unwrap_or(open_time_ms + interval_ms - 1)
    } else {
        row.get(5)
            .and_then(|value| value_u64(Some(value)))
            .unwrap_or(open_time_ms + interval_ms - 1)
    };
    Ok(KlineBar {
        exchange: exchange.to_string(),
        market: market.to_string(),
        symbol: symbol.to_ascii_uppercase(),
        interval: interval.to_string(),
        open_time_ms,
        close_time_ms,
        open: value_f64(row.get(1)).context("missing open")?,
        high: value_f64(row.get(2)).context("missing high")?,
        low: value_f64(row.get(3)).context("missing low")?,
        close: value_f64(row.get(4)).context("missing close")?,
        volume: has_volume.then(|| value_f64(row.get(5))).flatten(),
        source: source.to_string(),
        updated_at_ms: now_ms(),
    })
}

fn parse_okx_array_row(
    exchange: &str,
    market: &str,
    symbol: &str,
    interval: &str,
    source: &str,
    row: Value,
    has_volume: bool,
) -> Result<KlineBar> {
    let row = row.as_array().context("okx candle row is not array")?;
    if row.len() < 5 {
        bail!("short okx candle row");
    }
    let open_time_ms = value_u64(row.first()).context("missing open time")?;
    let interval_ms = interval_to_ms(interval).context("unsupported interval")?;
    Ok(KlineBar {
        exchange: exchange.to_string(),
        market: market.to_string(),
        symbol: symbol.to_ascii_uppercase(),
        interval: interval.to_string(),
        open_time_ms,
        close_time_ms: open_time_ms + interval_ms - 1,
        open: value_f64(row.get(1)).context("missing open")?,
        high: value_f64(row.get(2)).context("missing high")?,
        low: value_f64(row.get(3)).context("missing low")?,
        close: value_f64(row.get(4)).context("missing close")?,
        volume: has_volume.then(|| value_f64(row.get(5))).flatten(),
        source: source.to_string(),
        updated_at_ms: now_ms(),
    })
}

fn okx_inst_id(symbol: &str, market: &str) -> String {
    let symbol = symbol.trim().to_ascii_uppercase();
    let (base, quote) = symbol
        .strip_suffix("USDT")
        .map(|base| (base, "USDT"))
        .or_else(|| symbol.strip_suffix("USDC").map(|base| (base, "USDC")))
        .unwrap_or((symbol.as_str(), ""));
    if market == "spot" {
        format!("{base}-{quote}")
    } else {
        format!("{base}-{quote}-SWAP")
    }
}

fn okx_inst_family(symbol: &str) -> String {
    let inst_id = okx_inst_id(symbol, "perp");
    inst_id
        .strip_suffix("-SWAP")
        .unwrap_or(&inst_id)
        .to_string()
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_str()
            .and_then(|x| x.parse().ok())
            .or_else(|| value.as_f64())
    })
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        value
            .as_str()
            .and_then(|x| x.parse().ok())
            .or_else(|| value.as_u64())
    })
}

fn coinex_liquidation_side(raw_side: &str) -> &'static str {
    match raw_side {
        "long" => "sell",
        "short" => "buy",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binance_mark_row_without_volume() {
        let row = serde_json::json!([1000, "1.0", "2.0", "0.5", "1.5", 1999])
            .as_array()
            .expect("array")
            .clone();
        let bar = parse_binance_array_row("binance", "perp", "BTCUSDT", "1m", "test", row, false)
            .expect("bar");
        assert_eq!(bar.volume, None);
        assert_eq!(bar.close_time_ms, 1999);
    }

    #[test]
    fn maps_coinbase_product_and_granularity() {
        assert_eq!(coinbase_product_id("BTCUSDT"), "BTC-USD");
        assert_eq!(coinbase_product_id("BTC-USD"), "BTC-USD");
        assert_eq!(coinbase_granularity("1m"), Some(60));
        assert_eq!(coinbase_granularity("6h"), Some(21_600));
        assert_eq!(coinbase_granularity("4h"), None);
    }

    #[test]
    fn parses_coinbase_exchange_candle() {
        let row = serde_json::json!([1700000000, "99.0", "101.0", "100.0", "100.5", "12.3"])
            .as_array()
            .expect("array")
            .clone();
        let bar = parse_coinbase_row("BTCUSDT", "1m", row).expect("bar");
        assert_eq!(bar.exchange, "coinbase");
        assert_eq!(bar.market, "spot");
        assert_eq!(bar.symbol, "BTCUSDT");
        assert_eq!(bar.open_time_ms, 1_700_000_000_000);
        assert_eq!(bar.low, 99.0);
        assert_eq!(bar.high, 101.0);
        assert_eq!(bar.volume, Some(12.3));
    }

    #[test]
    fn normalizes_stablecoin_history_with_time_bounds_and_usd_fields() {
        let payload = serde_json::json!([
            {"date":"1700000000","totalCirculating":{"peggedUSD":100.0},"totalCirculatingUSD":{"peggedUSD":101.0}},
            {"date":"1700086400","totalCirculating":{"peggedUSD":102.0},"totalCirculatingUSD":{"peggedUSD":103.0}},
            {"date":"1700172800","totalCirculating":{"peggedUSD":104.0},"totalCirculatingUSD":{"peggedUSD":105.0}}
        ]);
        let query = HistoryStablecoinQuery {
            chain: Some("all".to_string()),
            start_ms: Some(1_700_086_400_000),
            end_ms: Some(1_700_172_800_000),
            limit: Some(10),
        };
        let rows = normalize_stablecoin_history(&payload, "all", &query).expect("stablecoin rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["ts_ms"], serde_json::json!(1_700_086_400_000_u64));
        assert_eq!(rows[0]["total_circulating_usd"], serde_json::json!(103.0));
        assert_eq!(rows[1]["chain"], serde_json::json!("all"));
    }

    #[test]
    fn stablecoin_history_limit_keeps_latest_rows() {
        let payload = serde_json::json!([
            {"date":"1700000000","totalCirculatingUSD":{"peggedUSD":101.0}},
            {"date":"1700086400","totalCirculatingUSD":{"peggedUSD":103.0}},
            {"date":"1700172800","totalCirculatingUSD":{"peggedUSD":105.0}}
        ]);
        let query = HistoryStablecoinQuery {
            limit: Some(2),
            ..Default::default()
        };
        let rows = normalize_stablecoin_history(&payload, "all", &query).expect("stablecoin rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["total_circulating_usd"], serde_json::json!(103.0));
        assert_eq!(rows[1]["total_circulating_usd"], serde_json::json!(105.0));
    }

    #[test]
    fn stablecoin_history_rejects_reversed_bounds_and_invalid_chain() {
        assert!(validate_stablecoin_chain("all/../private").is_err());
        assert!(validate_stablecoin_chain("Ethereum").is_ok());
    }

    #[test]
    fn normalizes_mining_history_to_millisecond_rows_and_keeps_latest_limit() {
        let payload = serde_json::json!({
            "hashrates": [
                {"timestamp": 100, "avgHashrate": 10.0},
                {"timestamp": 200, "avgHashrate": 20.0},
                {"timestamp": 300, "avgHashrate": 30.0}
            ],
            "difficulty": [
                {"time": 150, "height": 10, "difficulty": 100.0, "adjustment": 1.01},
                {"time": 250, "height": 20, "difficulty": 110.0, "adjustment": 1.10}
            ]
        });
        let (hashrates, adjustments, coverage) =
            normalize_mining_history(&payload, "1y", 2).expect("mining history");
        assert_eq!(hashrates.len(), 2);
        assert_eq!(hashrates[0]["ts_ms"], serde_json::json!(200_000_u64));
        assert_eq!(hashrates[1]["avg_hashrate_hs"], serde_json::json!(30.0));
        assert_eq!(adjustments[0]["height"], serde_json::json!(10));
        assert_eq!(coverage["status"], serde_json::json!("locally_limited"));
    }

    #[test]
    fn mining_history_window_rejects_unbounded_provider_paths() {
        assert_eq!(mining_history_window(Some("1y")).unwrap(), "1y");
        assert!(mining_history_window(Some("90d")).is_err());
    }

    #[test]
    fn okx_symbol_maps_perp_swap() {
        assert_eq!(okx_inst_id("BTCUSDT", "perp"), "BTC-USDT-SWAP");
        assert_eq!(okx_inst_id("BTCUSDT", "spot"), "BTC-USDT");
        assert_eq!(okx_inst_family("BTCUSDT"), "BTC-USDT");
    }

    #[test]
    fn maps_supported_bybit_open_interest_intervals() {
        assert_eq!(bybit_oi_interval("5m").unwrap(), "5min");
        assert_eq!(bybit_oi_interval("1h").unwrap(), "1h");
        assert!(bybit_oi_interval("2h").is_err());
    }

    #[test]
    fn maps_okx_open_interest_currency_and_period() {
        assert_eq!(okx_open_interest_currency("BTCUSDT"), "BTC");
        assert_eq!(okx_open_interest_currency("BTC-USDT-SWAP"), "BTC");
        assert_eq!(okx_open_interest_period("5m").unwrap(), "5m");
        assert_eq!(okx_open_interest_period("1h").unwrap(), "1H");
        assert!(okx_open_interest_period("15m").is_err());
        let row =
            normalize_okx_open_interest_row("BTCUSDT", &serde_json::json!(["1000", "12.5", "2.0"]))
                .expect("normalized OKX OI row");
        assert_eq!(row["ts_ms"], serde_json::json!(1000));
        assert_eq!(row["open_interest"], serde_json::json!(12.5));
        assert_eq!(row["unit"], serde_json::json!("provider_usd"));
    }

    #[test]
    fn maps_coinex_liquidation_position_to_aggressor_side() {
        assert_eq!(coinex_liquidation_side("long"), "sell");
        assert_eq!(coinex_liquidation_side("short"), "buy");
        assert_eq!(coinex_liquidation_side("other"), "unknown");
    }

    #[test]
    fn liquidation_coverage_never_claims_completeness() {
        assert_eq!(liquidation_coverage_status(5, 100), "bounded_single_page");
        assert_eq!(
            liquidation_coverage_status(100, 100),
            "provider_page_may_be_truncated"
        );
    }

    #[test]
    fn open_interest_coverage_never_claims_completeness() {
        assert_eq!(open_interest_coverage_status(2, 100), "bounded_single_page");
        assert_eq!(
            open_interest_coverage_status(100, 100),
            "provider_page_may_be_truncated"
        );
        let query = HistoryOpenInterestQuery {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            interval: Some("5m".to_string()),
            start_ms: Some(1),
            end_ms: Some(3),
            limit: Some(2),
            pages: None,
        };
        let rows = vec![
            serde_json::json!({"ts_ms": 1, "open_interest": 1.0}),
            serde_json::json!({"ts_ms": 3, "open_interest": 2.0}),
        ];
        let detail = open_interest_coverage_detail(&rows, &query);
        assert_eq!(detail["covered_start_ms"], serde_json::json!(1));
        assert_eq!(detail["covered_end_ms"], serde_json::json!(3));
        assert_eq!(
            detail["status"],
            serde_json::json!("provider_page_may_be_truncated")
        );
        assert_eq!(detail["requested_pages"], serde_json::json!(1));
    }

    #[test]
    fn history_windows_partition_requested_candle_range_without_overlap() {
        let windows = history_windows(Some(0), Some(9), 1, 4, 3);
        assert_eq!(windows, vec![(0, 3), (4, 7), (8, 9)]);
    }

    #[test]
    fn history_windows_default_range_is_bounded_by_page_count() {
        let windows = history_windows(None, Some(9), 1, 4, 2);
        assert_eq!(windows, vec![(2, 5), (6, 9)]);
    }

    #[test]
    fn normalizes_binance_taker_volume_and_imbalance() {
        let row = normalize_binance_taker_volume_row(serde_json::json!({
            "symbol": "BTCUSDT",
            "contractType": "PERPETUAL",
            "takerBuyVol": "60",
            "takerSellVol": "40",
            "takerBuyVolValue": "600000",
            "takerSellVolValue": "400000",
            "timestamp": 1234
        }))
        .expect("normalized row");
        assert_eq!(row["ts_ms"], serde_json::json!(1234));
        assert_eq!(row["total_volume"], serde_json::json!(100.0));
        let imbalance = row["imbalance"].as_f64().expect("numeric imbalance");
        assert!((imbalance - 0.2).abs() < 1e-12);
        assert_eq!(row["buy_sell_ratio"], serde_json::json!(1.5));
    }

    #[test]
    fn taker_volume_coverage_never_claims_completeness() {
        let query = HistoryTakerVolumeQuery {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            period: Some("5m".to_string()),
            start_ms: Some(1),
            end_ms: Some(3),
            limit: Some(2),
        };
        let rows = vec![
            serde_json::json!({"ts_ms": 1}),
            serde_json::json!({"ts_ms": 3}),
        ];
        let detail = taker_volume_coverage_detail(&rows, &query);
        assert_eq!(detail["covered_start_ms"], serde_json::json!(1));
        assert_eq!(detail["covered_end_ms"], serde_json::json!(3));
        assert_eq!(
            detail["status"],
            serde_json::json!("provider_page_may_be_truncated")
        );
    }

    #[test]
    fn normalizes_bybit_account_ratio_and_cursor_coverage() {
        let row = normalize_bybit_account_ratio_row(serde_json::json!({
            "symbol": "BTCUSDT",
            "buyRatio": "0.60",
            "sellRatio": "0.40",
            "timestamp": "1234"
        }))
        .expect("normalized row");
        assert_eq!(row["ts_ms"], serde_json::json!(1234));
        let imbalance = row["imbalance"].as_f64().expect("numeric imbalance");
        assert!((imbalance - 0.2).abs() < 1e-12);
        let long_short_ratio = row["long_short_ratio"]
            .as_f64()
            .expect("numeric long/short ratio");
        assert!((long_short_ratio - 1.5).abs() < 1e-12);
        let query = HistoryAccountRatioQuery {
            exchange: "bybit".to_string(),
            symbol: "BTCUSDT".to_string(),
            scope: None,
            period: Some("1h".to_string()),
            start_ms: None,
            end_ms: None,
            limit: Some(2),
        };
        let detail = account_ratio_coverage_detail(
            &[serde_json::json!({"ts_ms": 1234})],
            &query,
            Some("next"),
        );
        assert_eq!(detail["has_next_page"], serde_json::json!(true));
        assert_eq!(
            detail["status"],
            serde_json::json!("provider_page_may_be_truncated")
        );
    }

    #[test]
    fn normalizes_binance_top_trader_account_ratio_without_merging_semantics() {
        let row = normalize_binance_account_ratio_row(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "longAccount": "0.63",
                "shortAccount": "0.37",
                "longShortRatio": "1.70",
                "timestamp": 1234
            }),
            "binance_top_trader_account_ratio",
            "top_trader_account_share",
        )
        .expect("normalized Binance row");
        assert_eq!(
            row["source"],
            serde_json::json!("binance_top_trader_account_ratio")
        );
        assert_eq!(
            row["semantics"],
            serde_json::json!("top_trader_account_share")
        );
        assert_eq!(row["buy_ratio"], serde_json::json!(0.63));
        assert_eq!(row["sell_ratio"], serde_json::json!(0.37));
        assert_eq!(row["long_short_ratio"], serde_json::json!(1.70));
    }

    #[test]
    fn normalizes_binance_global_account_ratio_with_distinct_semantics() {
        let row = normalize_binance_account_ratio_row(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "longAccount": "0.55",
                "shortAccount": "0.45",
                "longShortRatio": "1.22",
                "timestamp": 5678
            }),
            "binance_global_account_ratio",
            "global_account_share",
        )
        .expect("normalized global Binance row");
        assert_eq!(
            row["source"],
            serde_json::json!("binance_global_account_ratio")
        );
        assert_eq!(row["semantics"], serde_json::json!("global_account_share"));
        assert_eq!(row["long_short_ratio"], serde_json::json!(1.22));
    }

    #[test]
    fn normalizes_binance_top_trader_position_ratio_without_merging_account_semantics() {
        let row = normalize_binance_account_ratio_row(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "longAccount": "0.70",
                "shortAccount": "0.30",
                "longShortRatio": "2.33",
                "timestamp": 9012
            }),
            "binance_top_trader_position_ratio",
            "top_trader_position_share",
        )
        .expect("normalized Binance position ratio");
        assert_eq!(
            row["source"],
            serde_json::json!("binance_top_trader_position_ratio")
        );
        assert_eq!(
            row["semantics"],
            serde_json::json!("top_trader_position_share")
        );
        assert_eq!(row["long_short_ratio"], serde_json::json!(2.33));
    }

    #[test]
    fn normalizes_documented_position_field_names_and_pair_identity() {
        let row = normalize_binance_account_ratio_row(
            serde_json::json!({
                "pair": "BTCUSD",
                "longPosition": "0.64",
                "shortPosition": "0.36",
                "timestamp": 9013
            }),
            "binance_top_trader_position_ratio",
            "top_trader_position_share",
        )
        .expect("normalized documented Binance position ratio");
        assert_eq!(row["symbol"], serde_json::json!("BTCUSD"));
        assert_eq!(row["buy_ratio"], serde_json::json!(0.64));
        assert_eq!(row["sell_ratio"], serde_json::json!(0.36));
    }

    #[test]
    fn normalizes_binance_basis_and_reports_bounded_coverage() {
        let row = normalize_binance_basis_row(serde_json::json!({
            "pair": "BTCUSDT",
            "contractType": "PERPETUAL",
            "indexPrice": "100.0",
            "futuresPrice": "100.5",
            "basis": "0.5",
            "basisRate": "0.005",
            "annualizedBasisRate": "0.5475",
            "timestamp": 1234
        }))
        .expect("normalized basis row");
        assert_eq!(row["symbol"], serde_json::json!("BTCUSDT"));
        assert_eq!(row["basis"], serde_json::json!(0.5));
        assert_eq!(row["basis_rate_bps"], serde_json::json!(50.0));
        assert_eq!(row["annualized_basis_rate_bps"], serde_json::json!(5475.0));
        let query = HistoryBasisQuery {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            contract_type: Some("PERPETUAL".to_string()),
            period: Some("1h".to_string()),
            start_ms: Some(1000),
            end_ms: Some(2000),
            limit: Some(30),
        };
        let detail = basis_coverage_detail(&[row], &query);
        assert_eq!(detail["covered_start_ms"], serde_json::json!(1234));
        assert_eq!(detail["status"], serde_json::json!("bounded_single_page"));
    }

    #[test]
    fn candle_coverage_reports_requested_and_observed_bounds() {
        assert_eq!(candle_coverage_status(2, 100), "bounded_single_page");
        assert_eq!(
            candle_coverage_status(100, 100),
            "provider_page_may_be_truncated"
        );
        let query = HistoryCandlesQuery {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            interval: Some("5m".to_string()),
            market: Some("perp".to_string()),
            candle_type: Some("perp".to_string()),
            start_ms: Some(10),
            end_ms: Some(30),
            limit: Some(2),
            pages: None,
            persist: None,
        };
        let first = KlineBar {
            exchange: "binance".to_string(),
            market: "perp".to_string(),
            symbol: "BTCUSDT".to_string(),
            interval: "5m".to_string(),
            open_time_ms: 10,
            close_time_ms: 1,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: None,
            source: "test".to_string(),
            updated_at_ms: 1,
        };
        let rows = vec![
            first.clone(),
            KlineBar {
                open_time_ms: 30,
                ..first
            },
        ];
        let detail = candle_coverage_detail(&rows, &query);
        assert_eq!(detail["covered_start_ms"], serde_json::json!(10));
        assert_eq!(detail["covered_end_ms"], serde_json::json!(30));
        assert_eq!(detail["requested_pages"], serde_json::json!(1));
        assert_eq!(
            detail["status"],
            serde_json::json!("provider_page_may_be_truncated")
        );
        let mut coinbase_query = query;
        coinbase_query.exchange = "coinbase".to_string();
        coinbase_query.limit = Some(500);
        let coinbase_detail = candle_coverage_detail(&rows, &coinbase_query);
        assert_eq!(coinbase_detail["page_limit"], serde_json::json!(300));
    }

    #[test]
    fn funding_schedule_exposes_point_in_time_adjacent_intervals() {
        let rows = [1000_u64, 28_801_000, 43_201_000]
            .into_iter()
            .map(|open_time_ms| KlineBar {
                exchange: "test".to_string(),
                market: "perp".to_string(),
                symbol: "BTCUSDT".to_string(),
                interval: "provider".to_string(),
                open_time_ms,
                close_time_ms: open_time_ms,
                open: 0.0,
                high: 0.0,
                low: 0.0,
                close: 0.0,
                volume: None,
                source: "test".to_string(),
                updated_at_ms: 1,
            })
            .collect::<Vec<_>>();
        let schedule = funding_schedule(&rows);
        assert_eq!(
            schedule["observed_intervals_ms"],
            serde_json::json!([14_400_000, 28_800_000])
        );
        assert_eq!(schedule["points"][0]["interval_ms"], 28_800_000);
        assert_eq!(schedule["points"][1]["interval_ms"], 14_400_000);
        assert_eq!(schedule["point_in_time"], true);
    }

    #[test]
    fn hyperliquid_coin_preserves_provider_symbol_semantics() {
        assert_eq!(hyperliquid_coin("BTCUSDT"), "BTC");
        assert_eq!(hyperliquid_coin("ETHUSDC"), "ETH");
        assert_eq!(hyperliquid_coin("xyz:XYZ100"), "XYZ:XYZ100");
        assert_eq!(hyperliquid_coin("BTC"), "BTC");
    }

    #[test]
    fn binance_trade_windows_are_hour_bounded_and_report_requested_span() {
        let (windows, requested) = binance_trade_windows(Some(0), Some(7_200_000), 2);
        assert_eq!(requested, 3);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0], (Some(0), Some(3_599_999)));
        assert_eq!(windows[1], (Some(3_600_000), Some(7_199_999)));
    }

    #[test]
    fn unbounded_single_trade_page_preserves_provider_latest_query() {
        let (windows, requested) = binance_trade_windows(None, None, 1);
        assert_eq!(windows, vec![(None, None)]);
        assert_eq!(requested, 1);
    }

    #[test]
    fn trade_coverage_status_exposes_partial_and_truncated_pages() {
        let partial = HistoricalTradesResult {
            rows: Vec::new(),
            requested_windows: 3,
            completed_windows: 2,
            truncated_windows: 0,
            requested_start_ms: None,
            requested_end_ms: None,
            covered_start_ms: None,
            covered_end_ms: None,
            page_limit: 1000,
        };
        assert_eq!(partial.coverage_status(), "partial_requested_windows");
        let truncated = HistoricalTradesResult {
            rows: Vec::new(),
            requested_windows: 1,
            completed_windows: 1,
            truncated_windows: 1,
            requested_start_ms: None,
            requested_end_ms: None,
            covered_start_ms: None,
            covered_end_ms: None,
            page_limit: 1000,
        };
        assert_eq!(
            truncated.coverage_status(),
            "provider_page_may_be_truncated"
        );
    }

    #[test]
    fn normalizes_bybit_historical_volatility_and_coverage() {
        let row = normalize_bybit_historical_volatility_row(
            serde_json::json!({"period": 30, "value": "0.45", "time": "1234"}),
            "BTC",
            "USD",
            7,
        )
        .expect("normalized volatility row");
        assert_eq!(row["base_coin"], serde_json::json!("BTC"));
        assert_eq!(row["quote_coin"], serde_json::json!("USD"));
        assert_eq!(row["period_days"], serde_json::json!(30));
        assert_eq!(row["volatility"], serde_json::json!(0.45));
        let query = HistoryHistoricalVolatilityQuery {
            exchange: "bybit".to_string(),
            base_coin: Some("BTC".to_string()),
            quote_coin: Some("USD".to_string()),
            period: Some(30),
            start_ms: Some(1000),
            end_ms: Some(2000),
        };
        let detail = historical_volatility_coverage_detail(&[row], &query);
        assert_eq!(detail["returned_rows"], serde_json::json!(1));
        assert_eq!(detail["covered_start_ms"], serde_json::json!(1234));
        assert_eq!(detail["status"], serde_json::json!("bounded_single_page"));
    }

    #[test]
    fn normalizes_deribit_volatility_index_ohlc_and_continuation() {
        let row = normalize_deribit_volatility_index_row(
            &serde_json::json!([1234, "40.0", 45.0, 39.5, "42.0"]),
            "BTC",
            "3600",
        )
        .expect("normalized Deribit volatility-index row");
        assert_eq!(row["exchange"], serde_json::json!("deribit"));
        assert_eq!(row["currency"], serde_json::json!("BTC"));
        assert_eq!(row["ts_ms"], serde_json::json!(1234));
        assert_eq!(row["close"], serde_json::json!(42.0));
        let query = HistoryVolatilityIndexQuery {
            currency: "BTC".to_string(),
            resolution: Some("3600".to_string()),
            start_ms: Some(1000),
            end_ms: Some(2000),
            limit: Some(10),
        };
        let detail = volatility_index_coverage_detail(&[row], &query, Some(3000));
        assert_eq!(detail["returned_rows"], serde_json::json!(1));
        assert_eq!(detail["covered_start_ms"], serde_json::json!(1234));
        assert_eq!(detail["continuation"], serde_json::json!(3000));
        assert_eq!(
            detail["status"],
            serde_json::json!("provider_page_may_be_truncated")
        );
    }

    #[test]
    fn rejects_invalid_deribit_volatility_index_candle() {
        let error = normalize_deribit_volatility_index_row(
            &serde_json::json!([1234, 40.0, 45.0]),
            "BTC",
            "3600",
        )
        .expect_err("short volatility-index candle must be rejected");
        assert!(error.to_string().contains("must contain timestamp"));
    }
}
