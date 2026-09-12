//! Explicit research identity. Display tickers never establish fungibility.
use serde::{Deserialize, Serialize};

use super::schema::{AssetClass, ProductType};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    pub id: String,
    pub venue: String,
    pub symbol: String,
    pub asset_class: AssetClass,
    pub product_type: ProductType,
    /// Registry-qualified identity, e.g. chain:contract, ISIN or currency ID.
    pub base_asset_id: String,
    pub quote_asset_id: String,
    pub quantity_unit: String,
    pub price_unit: String,
    pub contract_multiplier: f64,
    pub expiry_ms: Option<u64>,
    pub chain: Option<String>,
    pub contract_address: Option<String>,
    pub issuer: Option<String>,
    pub settlement: Option<String>,
}

impl Instrument {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("id", &self.id),
            ("venue", &self.venue),
            ("symbol", &self.symbol),
            ("base_asset_id", &self.base_asset_id),
            ("quote_asset_id", &self.quote_asset_id),
            ("quantity_unit", &self.quantity_unit),
            ("price_unit", &self.price_unit),
        ] {
            if value.trim().is_empty() || value.len() > 256 {
                return Err(format!("instrument {name} must contain 1..256 bytes"));
            }
        }
        if !self.contract_multiplier.is_finite() || self.contract_multiplier <= 0.0 {
            return Err("contract_multiplier must be finite and positive".into());
        }
        if self.chain.is_some() != self.contract_address.is_some() {
            return Err("chain and contract_address must be supplied together".into());
        }
        if matches!(self.product_type, ProductType::Future | ProductType::Option)
            && self.expiry_ms.is_none_or(|t| t == 0)
        {
            return Err("dated derivatives require expiry_ms".into());
        }
        Ok(())
    }

    /// Conservative same-asset spot model. Other relationships need other models.
    pub fn same_spot_asset(&self, other: &Self) -> bool {
        self.product_type == ProductType::Spot
            && other.product_type == ProductType::Spot
            && self.asset_class == other.asset_class
            && self.base_asset_id == other.base_asset_id
            && self.quote_asset_id == other.quote_asset_id
            && self.quantity_unit == other.quantity_unit
            && self.price_unit == other.price_unit
            && self.contract_multiplier == other.contract_multiplier
            && self.chain == other.chain
            && self.contract_address == other.contract_address
            && self.issuer == other.issuer
            && self.settlement == other.settlement
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    SameAsset,
    Convertible,
    Hedge,
    Correlated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRelationship {
    pub left_instrument: String,
    pub right_instrument: String,
    pub kind: RelationshipKind,
    pub evidence: String,
    pub known_at_ms: u64,
    pub valid_until_ms: Option<u64>,
}
