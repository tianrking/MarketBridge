//! Bounded candidate screening. Ranking never implies independent, additive fills.
use crate::research_engine::{self, ScanRequest, ScanResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub evidence: ScanRequest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub as_of_ms: u64,
    pub min_net_bps: f64,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Serialize)]
pub struct CandidateResult {
    pub id: String,
    pub result: Option<ScanResult>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RankedPoint {
    pub candidate_id: String,
    pub point_index: usize,
    pub base_quantity: f64,
    pub quote_asset_id: String,
    pub conditional_net_quote: f64,
    pub net_bps_on_buy_notional: f64,
}

#[derive(Debug, Serialize)]
pub struct BatchResult {
    pub model_version: &'static str,
    pub as_of_ms: u64,
    pub min_net_bps: f64,
    pub candidates: Vec<CandidateResult>,
    pub ranking: Vec<RankedPoint>,
    pub limitations: Vec<&'static str>,
}

pub fn validate_header<'a>(
    as_of: u64,
    minimum: f64,
    ids: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    if as_of == 0 || !minimum.is_finite() || minimum < 0.0 {
        return Err("positive as_of_ms and finite nonnegative min_net_bps required".into());
    }
    let mut seen = HashSet::new();
    let mut count = 0;
    for id in ids {
        count += 1;
        if id.trim().is_empty() || id.len() > 256 || !seen.insert(id) {
            return Err("candidate IDs must be unique and contain 1..256 bytes".into());
        }
    }
    if !(1..=64).contains(&count) {
        return Err("batch requires 1..64 candidates".into());
    }
    Ok(())
}

pub fn evaluate(request: &BatchRequest) -> Result<BatchResult, String> {
    validate_header(
        request.as_of_ms,
        request.min_net_bps,
        request.candidates.iter().map(|c| c.id.as_str()),
    )?;
    let candidates = request
        .candidates
        .iter()
        .map(|c| {
            let result = if c.evidence.as_of_ms == request.as_of_ms {
                research_engine::scan(&c.evidence)
            } else {
                Err("candidate decision time differs from batch as_of_ms".into())
            };
            candidate_result(c.id.clone(), result)
        })
        .collect();
    Ok(rank(request.as_of_ms, request.min_net_bps, candidates))
}

pub fn candidate_result(id: String, result: Result<ScanResult, String>) -> CandidateResult {
    match result {
        Ok(result) => CandidateResult {
            id,
            result: Some(result),
            error: None,
        },
        Err(error) => CandidateResult {
            id,
            result: None,
            error: Some(error),
        },
    }
}

pub fn rank(as_of_ms: u64, min_net_bps: f64, candidates: Vec<CandidateResult>) -> BatchResult {
    let mut ranking = Vec::new();
    for candidate in &candidates {
        let Some(result) = &candidate.result else {
            continue;
        };
        for (index, point) in result.points.iter().enumerate() {
            if let Some((net, bps)) = point
                .conditional_net_quote
                .zip(point.net_bps_on_buy_notional)
                .filter(|(net, bps)| *bps >= min_net_bps && *net > 0.0)
            {
                ranking.push(RankedPoint {
                    candidate_id: candidate.id.clone(),
                    point_index: index,
                    base_quantity: point.base_quantity,
                    quote_asset_id: result.quote_asset_id.clone(),
                    conditional_net_quote: net,
                    net_bps_on_buy_notional: bps,
                });
            }
        }
    }
    ranking.sort_by(|a, b| {
        b.net_bps_on_buy_notional
            .total_cmp(&a.net_bps_on_buy_notional)
            .then_with(|| a.candidate_id.cmp(&b.candidate_id))
            .then(a.point_index.cmp(&b.point_index))
    });
    BatchResult {
        model_version: "candidate-screen/v1",
        as_of_ms,
        min_net_bps,
        candidates,
        ranking,
        limitations: vec![
            "ranking uses conditional bps, not risk-adjusted return or annualized yield",
            "rows and sizes are alternatives; shared liquidity and inventory prevent adding profits",
            "quote currencies remain separate; no FX conversion or total portfolio profit inferred",
            "no automatic discovery of asset equivalence, orders or guaranteed fills",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(id: &str) -> Candidate {
        let mut evidence = crate::research_engine::tests::fixture();
        evidence.costs.buy_fee_bps = Some(0.0);
        evidence.costs.sell_fee_bps = Some(0.0);
        Candidate {
            id: id.into(),
            evidence,
        }
    }
    #[test]
    fn ranks_after_costs_and_retains_reference_rows() {
        let a = candidate("a");
        let mut b = candidate("b");
        b.evidence.costs.buy_fee_bps = Some(1.0);
        let mut c = candidate("unknown");
        c.evidence.costs.sell_fee_bps = None;
        let result = evaluate(&BatchRequest {
            as_of_ms: a.evidence.as_of_ms,
            min_net_bps: 0.0,
            candidates: vec![b, c, a],
        })
        .unwrap();
        assert_eq!(result.candidates.len(), 3);
        assert_eq!(result.ranking.len(), 2);
        assert_eq!(result.ranking[0].candidate_id, "a");
        assert_eq!(result.ranking[1].candidate_id, "b");
    }
    #[test]
    fn invalid_candidates_are_visible_and_do_not_hide_valid_ones() {
        let a = candidate("good");
        let mut b = candidate("invalid");
        b.evidence.buy.asks.clear();
        let mut c = candidate("other-time");
        c.evidence.as_of_ms += 1;
        let result = evaluate(&BatchRequest {
            as_of_ms: a.evidence.as_of_ms,
            min_net_bps: 0.0,
            candidates: vec![a, b, c],
        })
        .unwrap();
        assert_eq!(result.ranking.len(), 1);
        assert!(result.candidates[1].error.is_some());
        assert!(result.candidates[2].error.is_some());
    }
    #[test]
    fn ties_are_stable_and_bad_batch_headers_fail() {
        let a = candidate("a");
        let b = candidate("b");
        let mut request = BatchRequest {
            as_of_ms: a.evidence.as_of_ms,
            min_net_bps: 0.0,
            candidates: vec![b, a],
        };
        assert_eq!(evaluate(&request).unwrap().ranking[0].candidate_id, "a");
        request.candidates[1].id = "b".into();
        assert!(evaluate(&request).is_err());
        request.candidates.clear();
        assert!(evaluate(&request).is_err());
    }
}
