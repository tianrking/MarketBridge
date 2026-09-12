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

fn live_evidence(state: &ApiState, instrument: Instrument) -> Result<BookEvidence, String> {
    instrument.validate()?;
    let observed = state
        .bus
        .order_book_observation(&instrument.venue, &instrument.symbol)
        .ok_or_else(|| {
            format!(
                "no cached spot book for {}:{}",
                instrument.venue, instrument.symbol
            )
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
    let buy = live_evidence(&state, request.buy).map_err(invalid)?;
    let sell = live_evidence(&state, request.sell).map_err(invalid)?;
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

pub async fn scan_batch(
    Json(request): Json<crate::opportunity_scan::BatchRequest>,
) -> Result<Json<crate::opportunity_scan::BatchResult>, ValidationError> {
    let permit = RESEARCH_WORKERS.try_acquire().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"research workers busy"})),
        )
    })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::opportunity_scan::evaluate(&request)
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveCandidate {
    id: String,
    route: LiveScanRequest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveBatchRequest {
    min_net_bps: f64,
    candidates: Vec<LiveCandidate>,
}

pub async fn scan_live_batch(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<LiveBatchRequest>,
) -> Result<Json<crate::opportunity_scan::BatchResult>, ValidationError> {
    use crate::opportunity_scan::{candidate_result, rank, validate_header};
    validate_header(
        now_ms(),
        request.min_net_bps,
        request.candidates.iter().map(|c| c.id.as_str()),
    )
    .map_err(invalid)?;
    let permit = RESEARCH_WORKERS.try_acquire().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"research workers busy"})),
        )
    })?;
    // One cached observation per venue/native symbol in this request. The cache
    // is not a cross-venue atomic snapshot; source skew is checked by the model.
    let mut cache = std::collections::HashMap::new();
    let mut prepared = Vec::new();
    for candidate in request.candidates {
        let route = candidate.route;
        let mut load = |instrument: Instrument| {
            instrument.validate()?;
            let key = (instrument.venue.clone(), instrument.symbol.clone());
            let cached = cache
                .entry(key)
                .or_insert_with(|| live_evidence(&state, instrument.clone()));
            cached.clone().map(|mut evidence| {
                evidence.instrument = instrument;
                evidence
            })
        };
        let buy = load(route.buy);
        let sell = load(route.sell);
        prepared.push((
            candidate.id,
            buy.and_then(|buy| sell.map(|sell| (buy, sell)))
                .map(|(buy, sell)| ScanRequest {
                    as_of_ms: 0,
                    max_age_ms: route.max_age_ms,
                    max_skew_ms: route.max_skew_ms,
                    relationship: route.relationship,
                    quantities: route.quantities,
                    costs: route.costs,
                    buy,
                    sell,
                }),
        ));
    }
    let as_of_ms = now_ms();
    tokio::task::spawn_blocking(move || {
        let _permit=permit;
        let rows=prepared.into_iter().map(|(id,evidence)| {
            let result=evidence.and_then(|mut evidence| {evidence.as_of_ms=as_of_ms;research_engine::scan(&evidence)});
            candidate_result(id,result)
        }).collect();
        let mut result=rank(as_of_ms,request.min_net_bps,rows);
        result.limitations.push("cached observations are not atomic across venues; legacy source time may be receipt time");
        result
    }).await.map(Json).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"error":"research worker failed"}))))
}

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
