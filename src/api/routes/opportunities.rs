use crate::research_engine::{self, ReplayRequest, ReplayResult, ScanRequest, ScanResult};
use crate::{
    api::ApiState,
    core::instrument::{AssetRelationship, Instrument},
    domains::market::quote::QuoteKind,
    research_engine::{BookEvidence, CostAssumptions},
    types::now_ms,
};
use axum::extract::State;
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveScanRequest {
    pub buy: Instrument,
    pub sell: Instrument,
    pub relationship: AssetRelationship,
    pub quantities: Vec<f64>,
    pub costs: CostAssumptions,
    pub max_age_ms: u64,
    pub max_skew_ms: u64,
}

fn live_evidence(
    state: &ApiState,
    instrument: Instrument,
) -> Result<BookEvidence, ValidationError> {
    let observed = state
        .bus
        .order_book_observation(&instrument.venue, &instrument.symbol)
        .ok_or_else(|| {
            invalid(format!(
                "no cached spot book for {}:{}",
                instrument.venue, instrument.symbol
            ))
        })?;
    // These existing spot adapters subscribe to full top-N snapshots (Binance
    // depth20 / OKX books5). Other adapters may emit deltas; never promote them.
    let complete = matches!(observed.book.exchange, "binance" | "okx");
    Ok(BookEvidence {
        observation_id: format!(
            "cache:{}:{}:{}:{}",
            observed.book.exchange,
            observed.book.symbol,
            observed.received_at_ms,
            observed.sequence
        ),
        instrument,
        quote_kind: if complete {
            QuoteKind::ObservedBook
        } else {
            QuoteKind::Reference
        },
        source_time_ms: observed.book.ts_ms,
        received_at_ms: observed.received_at_ms,
        complete,
        bids: observed.book.bids,
        asks: observed.book.asks,
    })
}

pub async fn evaluate_live(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<LiveScanRequest>,
) -> Result<Json<ScanResult>, ValidationError> {
    request.buy.validate().map_err(invalid)?;
    request.sell.validate().map_err(invalid)?;
    let buy = live_evidence(&state, request.buy)?;
    let sell = live_evidence(&state, request.sell)?;
    let request = ScanRequest {
        as_of_ms: now_ms(),
        max_age_ms: request.max_age_ms,
        max_skew_ms: request.max_skew_ms,
        relationship: request.relationship,
        quantities: request.quantities,
        costs: request.costs,
        buy,
        sell,
    };
    let mut result = research_engine::scan(&request).map_err(invalid)?;
    result.limitations.push(
        "legacy source time may be local receipt time; source-clock precision is not guaranteed",
    );
    result
        .limitations
        .push("identity and costs supplied by caller; no automatic fungibility registry");
    Ok(Json(result))
}

type ValidationError = (StatusCode, Json<Value>);
static RESEARCH_WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub async fn paper(
    Json(request): Json<crate::paper::PaperRequest>,
) -> Result<Json<crate::paper::PaperResult>, ValidationError> {
    let permit = RESEARCH_WORKERS.try_acquire().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"research workers busy"})),
        )
    })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::paper::simulate(&request)
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"research worker failed"})),
        )
    })?
    .map(Json)
    .map_err(invalid)
}

fn invalid(error: String) -> ValidationError {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"error": error, "orders_supported": false})),
    )
}

pub async fn evaluate(
    Json(request): Json<ScanRequest>,
) -> Result<Json<ScanResult>, ValidationError> {
    research_engine::scan(&request).map(Json).map_err(invalid)
}

pub async fn replay(
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResult>, ValidationError> {
    let permit = RESEARCH_WORKERS.try_acquire().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"research workers busy"})),
        )
    })?;
    // Bound both individual work and concurrent workers; don't queue unlimited jobs.
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        research_engine::replay(&request)
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"research worker failed"})),
        )
    })?
    .map(Json)
    .map_err(invalid)
}
