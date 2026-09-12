//! Reference analytics only: no fill, convergence or future funding promises.
use crate::{
    core::{
        instrument::{AssetRelationship, Instrument, RelationshipKind},
        schema::ProductType,
    },
    types::timestamp_is_fresh,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencePoint {
    pub instrument: Instrument,
    pub price: f64,
    pub known_at_ms: u64,
    pub source_time_ms: u64,
    pub evidence: String,
}
impl ReferencePoint {
    fn validate(&self, at: u64, ttl: u64) -> Result<(), String> {
        self.instrument.validate()?;
        if self.known_at_ms == 0
            || self.known_at_ms > at
            || !timestamp_is_fresh(self.source_time_ms, at, ttl)
            || at.saturating_sub(self.known_at_ms) > ttl
            || self.source_time_ms > self.known_at_ms.saturating_add(1000)
            || self.evidence.trim().is_empty()
            || !self.price.is_finite()
            || self.price <= 0.0
        {
            return Err("invalid or stale reference evidence".into());
        }
        Ok(())
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativeRequest {
    pub model: String,
    pub as_of_ms: u64,
    pub max_age_ms: u64,
    pub max_skew_ms: u64,
    pub left: ReferencePoint,
    pub right: ReferencePoint,
    pub relationship: AssetRelationship,
    pub carry_cost_per_unit: Option<f64>,
}
pub fn relative(req: &RelativeRequest) -> Result<Value, String> {
    if req.as_of_ms == 0 || req.max_age_ms == 0 {
        return Err("positive decision time/TTL required".into());
    }
    req.left.validate(req.as_of_ms, req.max_age_ms)?;
    req.right.validate(req.as_of_ms, req.max_age_ms)?;
    let (a, b) = (&req.left.instrument, &req.right.instrument);
    let r = &req.relationship;
    if r.left_instrument != a.id
        || r.right_instrument != b.id
        || r.known_at_ms == 0
        || r.known_at_ms > req.as_of_ms
        || r.valid_until_ms.is_some_and(|t| t < req.as_of_ms)
        || r.evidence.trim().is_empty()
    {
        return Err("unverified relationship".into());
    }
    if a.quote_asset_id != b.quote_asset_id
        || a.price_unit != b.price_unit
        || req.left.source_time_ms.abs_diff(req.right.source_time_ms) > req.max_skew_ms
    {
        return Err("quote units or observation times differ".into());
    }
    if req
        .carry_cost_per_unit
        .is_some_and(|c| !c.is_finite() || c < 0.0)
    {
        return Err("invalid carry cost".into());
    }
    let difference = req.right.price - req.left.price;
    let bps = difference / req.left.price * 10000.0;
    if !difference.is_finite() || !bps.is_finite() {
        return Err("numeric overflow".into());
    }
    let mut out = json!({"model_version":req.model,"as_of_ms":req.as_of_ms,"status":"reference_only","quote_asset_id":a.quote_asset_id,"price_unit":a.price_unit,"difference_per_unit":difference,"difference_bps":bps,"conditional_net_profit":null,"limitations":["reference prices are not executable legs","identity is caller-attested; no conversion or convergence guarantee"]});
    match req.model.as_str() {
        "spot-derivative-basis/v1" => {
            if a.product_type != ProductType::Spot
                || !matches!(b.product_type, ProductType::Future | ProductType::Perp)
                || a.base_asset_id != b.base_asset_id
                || r.kind != RelationshipKind::Hedge
            {
                return Err(
                    "basis requires spot, matching derivative underlying and hedge relationship"
                        .into(),
                );
            }
            out["base_units_per_derivative_contract"] = json!(b.contract_multiplier);
            if let Some(expiry) = b.expiry_ms {
                if expiry <= req.as_of_ms {
                    return Err("derivative expired".into());
                }
                let annualized = bps * (365.0 * 86400000.0 / (expiry - req.as_of_ms) as f64);
                if !annualized.is_finite() {
                    return Err("annualization overflow".into());
                }
                out["simple_annualized_reference_basis_bps"] = json!(annualized);
            }
            let adjusted = req.carry_cost_per_unit.map(|c| difference - c);
            if adjusted.is_some_and(|v| !v.is_finite()) {
                return Err("carry arithmetic overflow".into());
            }
            out["after_assumed_carry_difference_per_unit"] = json!(adjusted);
        }
        "unit-premium/v1" => {
            if !matches!(
                r.kind,
                RelationshipKind::Convertible
                    | RelationshipKind::Correlated
                    | RelationshipKind::Hedge
            ) {
                return Err("premium requires explicit non-fungible relationship".into());
            }
            out["conversion_guaranteed"] = json!(false);
        }
        _ => return Err("unsupported relative-value model".into()),
    }
    Ok(out)
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingLeg {
    pub instrument: Instrument,
    pub rate: f64,
    pub interval_ms: u64,
    pub known_at_ms: u64,
    pub evidence: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingRequest {
    pub as_of_ms: u64,
    pub max_age_ms: u64,
    pub long: FundingLeg,
    pub short: FundingLeg,
}
pub fn funding(req: &FundingRequest) -> Result<Value, String> {
    for l in [&req.long, &req.short] {
        l.instrument.validate()?;
        if req.as_of_ms == 0
            || req.max_age_ms == 0
            || l.instrument.product_type != ProductType::Perp
            || !l.rate.is_finite()
            || l.rate.abs() > 1.0
            || l.interval_ms == 0
            || l.known_at_ms == 0
            || l.known_at_ms > req.as_of_ms
            || req.as_of_ms - l.known_at_ms > req.max_age_ms
            || l.evidence.trim().is_empty()
        {
            return Err("invalid funding evidence".into());
        }
    }
    if req.long.instrument.base_asset_id != req.short.instrument.base_asset_id
        || req.long.instrument.quote_asset_id != req.short.instrument.quote_asset_id
        || req.long.instrument.price_unit != req.short.instrument.price_unit
    {
        return Err("funding underlyings/quote units differ".into());
    }
    let difference = (req.short.rate / req.short.interval_ms as f64
        - req.long.rate / req.long.interval_ms as f64)
        * 3600000.0;
    Ok(
        json!({"model_version":"funding-rate-comparison/v1","status":"reference_only","hourly_rate_difference":difference,"positive_rate_convention":"long_pays_short","conditional_net_profit":null,"limitations":["linear rate normalization, not future funding prediction","equal underlying exposure assumed; inverse/quanto settlement not modeled","fees, margin, borrow and liquidation not modeled"]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basis_checks_units_expiry_and_does_not_claim_profit() {
        let f = crate::research_engine::tests::fixture();
        let mut future = f.sell.instrument;
        future.product_type = ProductType::Future;
        future.expiry_ms = Some(100000000);
        future.contract_multiplier = 100.0;
        let mut rel = f.relationship;
        rel.kind = RelationshipKind::Hedge;
        let mut req = RelativeRequest {
            model: "spot-derivative-basis/v1".into(),
            as_of_ms: 10020,
            max_age_ms: 1000,
            max_skew_ms: 100,
            left: ReferencePoint {
                instrument: f.buy.instrument,
                price: 100.0,
                known_at_ms: 10000,
                source_time_ms: 10000,
                evidence: "fixture".into(),
            },
            right: ReferencePoint {
                instrument: future,
                price: 102.0,
                known_at_ms: 10000,
                source_time_ms: 10000,
                evidence: "fixture".into(),
            },
            relationship: rel,
            carry_cost_per_unit: Some(1.0),
        };
        let value = relative(&req).unwrap();
        assert_eq!(value["difference_bps"], 200.0);
        assert_eq!(value["after_assumed_carry_difference_per_unit"], 1.0);
        assert!(value["conditional_net_profit"].is_null());
        req.right.instrument.expiry_ms = Some(1);
        assert!(relative(&req).is_err());
    }
    #[test]
    fn funding_compares_intervals_not_raw_rates() {
        let f = crate::research_engine::tests::fixture();
        let mut a = f.buy.instrument;
        let mut b = f.sell.instrument;
        a.product_type = ProductType::Perp;
        b.product_type = ProductType::Perp;
        let req = FundingRequest {
            as_of_ms: 100,
            max_age_ms: 100,
            long: FundingLeg {
                instrument: a,
                rate: 0.0008,
                interval_ms: 28800000,
                known_at_ms: 100,
                evidence: "x".into(),
            },
            short: FundingLeg {
                instrument: b,
                rate: 0.0001,
                interval_ms: 3600000,
                known_at_ms: 100,
                evidence: "y".into(),
            },
        };
        assert!(
            funding(&req).unwrap()["hourly_rate_difference"]
                .as_f64()
                .unwrap()
                .abs()
                < 1e-15
        );
    }
}
