//! Versioned, caller-attested asset relationships. Never infer from ticker names.
use crate::core::instrument::{AssetRelationship, Instrument, RelationshipKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryRevision {
    pub id: String,
    pub known_at_ms: u64,
    pub evidence: String,
    pub instruments: Vec<Instrument>,
    pub relationships: Vec<AssetRelationship>,
}
impl RegistryRevision {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::research_store::valid_id(&self.id)
            || self.known_at_ms == 0
            || self.evidence.trim().is_empty()
        {
            return Err("registry ID, knowledge time and evidence required".into());
        }
        if self.instruments.is_empty()
            || self.instruments.len() > 512
            || self.relationships.len() > 2048
        {
            return Err(
                "registry requires 1..512 instruments and at most 2048 relationships".into(),
            );
        }
        let mut map = HashMap::new();
        for i in &self.instruments {
            i.validate()?;
            if map.insert(i.id.as_str(), i).is_some() {
                return Err("duplicate instrument ID".into());
            }
            for field in [&i.chain, &i.contract_address, &i.issuer, &i.settlement]
                .into_iter()
                .flatten()
            {
                if field.trim().is_empty() || field.len() > 512 {
                    return Err("empty or oversized optional identity field".into());
                }
            }
        }
        let mut edges = std::collections::HashSet::new();
        for r in &self.relationships {
            let a = map
                .get(r.left_instrument.as_str())
                .ok_or("unknown left instrument")?;
            let b = map
                .get(r.right_instrument.as_str())
                .ok_or("unknown right instrument")?;
            if a.id == b.id || !edges.insert((&r.left_instrument, &r.right_instrument)) {
                return Err("self or duplicate relationship".into());
            }
            if r.known_at_ms == 0
                || r.known_at_ms > self.known_at_ms
                || r.evidence.trim().is_empty()
                || r.valid_until_ms.is_some_and(|t| t < r.known_at_ms)
            {
                return Err("invalid relationship knowledge/expiry/evidence".into());
            }
            if r.kind == RelationshipKind::SameAsset && !a.same_spot_asset(b) {
                return Err("same_asset relationship conflicts with instrument semantics".into());
            }
        }
        Ok(())
    }
    pub fn pair(
        &self,
        left: &str,
        right: &str,
        as_of: u64,
    ) -> Result<(Instrument, Instrument, AssetRelationship), String> {
        self.validate()?;
        if self.known_at_ms > as_of {
            return Err("registry revision was not known at decision time".into());
        }
        let relation = self
            .relationships
            .iter()
            .find(|r| {
                r.left_instrument == left
                    && r.right_instrument == right
                    && r.known_at_ms <= as_of
                    && r.valid_until_ms.is_none_or(|t| t >= as_of)
            })
            .ok_or("no valid directed relationship")?;
        let find = |id: &str| {
            self.instruments
                .iter()
                .find(|i| i.id == id)
                .cloned()
                .ok_or("missing instrument".to_string())
        };
        Ok((find(left)?, find(right)?, relation.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_checks_identity_and_knowledge_time() {
        let fixture = crate::research_engine::tests::fixture();
        let mut r = RegistryRevision {
            id: "v1".into(),
            known_at_ms: fixture.relationship.known_at_ms,
            evidence: "fixture".into(),
            instruments: vec![fixture.buy.instrument, fixture.sell.instrument],
            relationships: vec![fixture.relationship],
        };
        r.validate().unwrap();
        assert!(
            r.pair(&r.instruments[0].id, &r.instruments[1].id, 0)
                .is_err()
        );
        assert!(
            r.pair(&r.instruments[0].id, &r.instruments[1].id, 100000)
                .is_ok()
        );
        r.instruments[1].quote_asset_id = "different".into();
        assert!(r.validate().is_err());
    }
}
