//! Read-only, evidence-first short-squeeze radar.
//!
//! This module deliberately does not emit trading instructions. It turns the
//! live symbol-state evidence into a ranked research queue and makes every
//! missing prerequisite explicit, so a sparse or stale feed cannot masquerade
//! as a usable signal.

use serde::Serialize;

use crate::strategy_state::{RollingChange, StrategySymbolState};

pub const MODEL_VERSION: &str = "squeeze-radar/v0";
const ONE_HOUR_MS: u64 = 60 * 60_000;
const FIFTEEN_MINUTES_MS: u64 = 15 * 60_000;

#[derive(Debug, Clone, Serialize)]
pub struct SqueezeScanResponse {
    pub model_version: &'static str,
    pub domain: &'static str,
    pub generated_at_ms: u64,
    pub max_data_age_ms: u64,
    pub minimum_score: i32,
    pub observed_symbols: usize,
    pub candidates: Vec<SqueezeCandidate>,
    pub limitations: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SqueezeCandidate {
    pub exchange: String,
    pub symbol: String,
    pub state: &'static str,
    pub score: i32,
    pub max_score: i32,
    pub data_quality: SqueezeDataQuality,
    pub evidence: Vec<String>,
    pub missing_evidence: Vec<String>,
    /// Raw source-derived state is retained beside the model decision so a
    /// caller can independently reproduce or reject the ranking.
    pub raw: StrategySymbolState,
}

#[derive(Debug, Clone, Serialize)]
pub struct SqueezeDataQuality {
    pub observed_at_ms: u64,
    pub age_ms: Option<u64>,
    pub fresh: bool,
    pub funding_fresh: bool,
    pub open_interest_fresh: bool,
    pub price_fresh: bool,
    pub order_book_fresh: bool,
    pub oi_1h_baseline: Option<RollingChange>,
    pub price_15m_baseline: Option<RollingChange>,
    pub funding_observations_24h: usize,
    pub ready_for_trigger: bool,
}

pub fn scan(
    states: Vec<StrategySymbolState>,
    now_ms: u64,
    max_data_age_ms: u64,
    minimum_score: i32,
    limit: usize,
) -> SqueezeScanResponse {
    let observed_symbols = states.len();
    let mut candidates = states
        .into_iter()
        .map(|state| evaluate(state, now_ms, max_data_age_ms))
        .filter(|candidate| candidate.score >= minimum_score || !candidate.data_quality.fresh)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .data_quality
            .ready_for_trigger
            .cmp(&left.data_quality.ready_for_trigger)
            .then(right.score.cmp(&left.score))
            .then(left.exchange.cmp(&right.exchange))
            .then(left.symbol.cmp(&right.symbol))
    });
    candidates.truncate(limit.clamp(1, 500));
    SqueezeScanResponse {
        model_version: MODEL_VERSION,
        domain: "read_only_squeeze_research",
        generated_at_ms: now_ms,
        max_data_age_ms,
        minimum_score,
        observed_symbols,
        candidates,
        limitations: vec![
            "A ranking is a research hypothesis, not a trade instruction or a profitability claim.",
            "Funding is not normalized across venue funding intervals or caps in v0; compare within a venue before using it as a factor.",
            "No issuer supply, borrow availability, deposit status, exchange reserve, wallet-label, liquidation-wall, or execution evidence is inferred.",
            "The state is process-local live observation; restart clears rolling baselines unless a separate replay dataset is supplied.",
        ],
    }
}

fn evaluate(raw: StrategySymbolState, now_ms: u64, max_data_age_ms: u64) -> SqueezeCandidate {
    let metrics = &raw.metrics;
    let age_ms = (raw.observed_at_ms > 0).then(|| now_ms.saturating_sub(raw.observed_at_ms));
    let fresh = age_ms.is_some_and(|age| age <= max_data_age_ms);
    let funding_fresh = timestamp_fresh(metrics.funding_observed_at_ms, now_ms, max_data_age_ms);
    let open_interest_fresh = timestamp_fresh(
        metrics.open_interest_observed_at_ms,
        now_ms,
        max_data_age_ms,
    );
    let price_fresh = timestamp_fresh(metrics.price_observed_at_ms, now_ms, max_data_age_ms);
    let order_book_fresh =
        timestamp_fresh(metrics.order_book_observed_at_ms, now_ms, max_data_age_ms);
    let oi_1h = change_for(&metrics.open_interest_changes, ONE_HOUR_MS);
    let price_15m = change_for(&metrics.price_changes, FIFTEEN_MINUTES_MS);
    let mut evidence = Vec::new();
    let mut missing = Vec::new();
    let mut score = 0;

    match metrics.funding_rate {
        Some(rate) if rate <= -0.0005 => {
            score += 2;
            evidence.push(format!("funding deeply negative: {:.4}%", rate * 100.0));
        }
        Some(rate) if rate < 0.0 => {
            score += 1;
            evidence.push(format!("funding negative: {:.4}%", rate * 100.0));
        }
        Some(_) => {}
        None => missing.push("funding rate unavailable".into()),
    }

    match oi_1h.as_ref().map(|change| change.change_pct) {
        Some(change) if change >= 15.0 => {
            score += 3;
            evidence.push(format!("OI +{change:.2}% over observed ~1h baseline"));
        }
        Some(change) if change >= 3.0 => {
            score += 1;
            evidence.push(format!("OI rising +{change:.2}% over observed ~1h baseline"));
        }
        Some(_) => {}
        None => missing.push("aligned 1h OI baseline unavailable; still warming up or sampling cadence is insufficient".into()),
    }

    match metrics.cvd_divergence.as_deref() {
        Some("spot_up_perp_down") => {
            score += 2;
            evidence.push("spot CVD positive while perpetual CVD negative".into());
        }
        Some(_) => {}
        None => missing.push("matched spot/perpetual CVD unavailable".into()),
    }

    if metrics.ofi_best_level_1m.is_some_and(|value| value > 0.0) {
        score += 1;
        evidence.push("best-level order-flow imbalance is buy-positive".into());
    } else if metrics.ofi_best_level_1m.is_none() {
        missing.push("perpetual order-book OFI unavailable".into());
    }

    if metrics
        .buy_liquidation_notional_15m
        .is_some_and(|value| value > 0.0)
    {
        score += 1;
        evidence.push("recent buy-side liquidations observed".into());
    }

    match price_15m.as_ref().map(|change| change.change_pct) {
        Some(change) if change > 0.0 => {
            score += 1;
            evidence.push(format!(
                "price is above its observed ~15m baseline: +{change:.2}%"
            ));
        }
        Some(_) => {}
        None => missing
            .push("aligned 15m price baseline unavailable; no right-side confirmation".into()),
    }

    if !fresh {
        missing.push(format!(
            "market data stale: age {}ms exceeds {}ms",
            age_ms.unwrap_or(u64::MAX),
            max_data_age_ms
        ));
    }
    if !funding_fresh {
        missing.push("funding observation is stale or has no source timestamp".into());
    }
    if !open_interest_fresh {
        missing.push("open-interest observation is stale or has no source timestamp".into());
    }
    if !price_fresh {
        missing.push("price observation is stale or has no source timestamp".into());
    }
    if !order_book_fresh {
        missing.push("order-book observation is stale or has no source timestamp".into());
    }
    let ready_for_trigger = fresh
        && funding_fresh
        && open_interest_fresh
        && price_fresh
        && order_book_fresh
        && metrics.funding_rate.is_some()
        && oi_1h.is_some()
        && price_15m.is_some()
        && metrics.cvd_divergence.is_some()
        && metrics.ofi_best_level_1m.is_some();
    let state = if !fresh {
        "stale_data"
    } else if !ready_for_trigger {
        "warming_up"
    } else if score >= 8
        && price_15m
            .as_ref()
            .is_some_and(|change| change.change_pct > 0.0)
    {
        "triggered_research_candidate"
    } else if score >= 5 {
        "armed_research_candidate"
    } else {
        "watch"
    };

    SqueezeCandidate {
        exchange: raw.exchange.clone(),
        symbol: raw.symbol.clone(),
        state,
        score,
        max_score: 10,
        data_quality: SqueezeDataQuality {
            observed_at_ms: raw.observed_at_ms,
            age_ms,
            fresh,
            funding_fresh,
            open_interest_fresh,
            price_fresh,
            order_book_fresh,
            oi_1h_baseline: oi_1h,
            price_15m_baseline: price_15m,
            funding_observations_24h: metrics.funding_observations_24h,
            ready_for_trigger,
        },
        evidence,
        missing_evidence: missing,
        raw,
    }
}

fn timestamp_fresh(timestamp: Option<u64>, now_ms: u64, max_data_age_ms: u64) -> bool {
    timestamp
        .is_some_and(|value| value <= now_ms && now_ms.saturating_sub(value) <= max_data_age_ms)
}

fn change_for(changes: &[RollingChange], window_ms: u64) -> Option<RollingChange> {
    changes
        .iter()
        .find(|change| change.window_ms == window_ms)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy_state::{RiskContext, StrategyLegState, StrategyMetrics};

    fn state(now: u64) -> StrategySymbolState {
        StrategySymbolState {
            exchange: "test".into(),
            symbol: "ABCUSDT".into(),
            generated_at_ms: now,
            observed_at_ms: now,
            metrics: StrategyMetrics {
                funding_rate: Some(-0.0006),
                funding_observed_at_ms: Some(now),
                open_interest_observed_at_ms: Some(now),
                price_observed_at_ms: Some(now),
                order_book_observed_at_ms: Some(now),
                open_interest_changes: vec![RollingChange {
                    window_ms: ONE_HOUR_MS,
                    actual_elapsed_ms: ONE_HOUR_MS,
                    change_pct: 20.0,
                    baseline_ts_ms: now.saturating_sub(ONE_HOUR_MS),
                    latest_ts_ms: now,
                }],
                price_changes: vec![RollingChange {
                    window_ms: FIFTEEN_MINUTES_MS,
                    actual_elapsed_ms: FIFTEEN_MINUTES_MS,
                    change_pct: 2.0,
                    baseline_ts_ms: now.saturating_sub(FIFTEEN_MINUTES_MS),
                    latest_ts_ms: now,
                }],
                funding_observations_24h: 4,
                cvd_divergence: Some("spot_up_perp_down".into()),
                ofi_best_level_1m: Some(1.0),
                buy_liquidation_notional_15m: Some(1.0),
                ..StrategyMetrics::default()
            },
            long_squeeze: leg(),
            short_exhaustion: leg(),
            risk_context: RiskContext::default(),
        }
    }

    fn leg() -> StrategyLegState {
        StrategyLegState {
            state: "neutral",
            score: 0,
            max_score: 0,
            progress: Vec::new(),
            reasons: Vec::new(),
            invalidation: Vec::new(),
        }
    }

    #[test]
    fn complete_confluence_is_a_triggered_research_candidate() {
        let report = scan(vec![state(10_000_000)], 10_000_000, 3_000, 0, 10);
        assert_eq!(report.candidates[0].state, "triggered_research_candidate");
        assert_eq!(report.candidates[0].score, 10);
        assert!(report.candidates[0].data_quality.ready_for_trigger);
    }

    #[test]
    fn stale_data_can_never_be_triggered() {
        let report = scan(vec![state(10_000_000)], 20_000_000, 3_000, 0, 10);
        assert_eq!(report.candidates[0].state, "stale_data");
        assert!(!report.candidates[0].data_quality.ready_for_trigger);
    }
}
