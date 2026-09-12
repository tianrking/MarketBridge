//! Prefunded taker-fill scenarios, including partial/unmatched legs.
//! No orders, borrowing, inferred fills, or settlement-grade money arithmetic.
use crate::research_engine::{self, BookEvidence, ReplayRequest, ScanRequest};
use crate::types::BookLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub buy_venue_quote: f64,
    pub buy_venue_base: f64,
    pub sell_venue_quote: f64,
    pub sell_venue_base: f64,
}

impl Inventory {
    fn valid(&self) -> bool {
        [
            self.buy_venue_quote,
            self.buy_venue_base,
            self.sell_venue_quote,
            self.sell_venue_base,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.0)
            && (self.buy_venue_quote + self.sell_venue_quote).is_finite()
            && (self.buy_venue_base + self.sell_venue_base).is_finite()
    }
    fn cash(&self) -> f64 {
        self.buy_venue_quote + self.sell_venue_quote
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperFrame {
    pub evidence: ScanRequest,
    pub size_index: usize,
    /// Explicit scenario assumptions, not probabilities or actual fill reports.
    pub buy_fill_fraction: f64,
    pub sell_fill_fraction: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperRequest {
    pub initial: Inventory,
    pub frames: Vec<PaperFrame>,
}

#[derive(Debug, Serialize)]
pub struct LedgerEntry {
    pub as_of_ms: u64,
    pub buy_observation_id: String,
    pub sell_observation_id: String,
    pub buy_filled_base: f64,
    pub sell_filled_base: f64,
    pub fees_quote: f64,
    pub other_cost_quote: f64,
    pub cash_change_quote: f64,
    pub residual_base_change: f64,
    pub inventory: Inventory,
    pub status: &'static str,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PaperResult {
    pub model_version: &'static str,
    pub quote_asset_id: String,
    pub base_asset_id: String,
    pub initial: Inventory,
    pub final_inventory: Inventory,
    pub net_cash_change_quote: f64,
    pub residual_base_change: f64,
    /// Only present when simulated base exposure has returned exactly to zero.
    pub closed_base_cash_pnl_quote: Option<f64>,
    pub entries: Vec<LedgerEntry>,
    pub limitations: Vec<&'static str>,
}

#[derive(Default)]
pub(crate) struct LiquidityUse {
    fingerprint: String,
    pub(crate) consumed_base: f64,
}

fn evidence_key(book: &BookEvidence, side: &str) -> String {
    serde_json::to_string(&(&book.instrument.venue, &book.observation_id, side))
        .expect("string tuple")
}

pub(crate) fn register_evidence(
    used: &mut HashMap<String, LiquidityUse>,
    book: &BookEvidence,
    side: &str,
) -> Result<String, String> {
    let key = evidence_key(book, side);
    let fingerprint = serde_json::to_string(book).map_err(|e| e.to_string())?;
    let entry = used.entry(key.clone()).or_insert_with(|| LiquidityUse {
        fingerprint: fingerprint.clone(),
        consumed_base: 0.0,
    });
    if entry.fingerprint != fingerprint {
        return Err("observation ID reused with different content".into());
    }
    Ok(key)
}

pub(crate) fn remaining_quote(
    levels: &[BookLevel],
    already_used: f64,
    quantity: f64,
) -> Option<f64> {
    if quantity == 0.0 {
        return Some(0.0);
    }
    let before = if already_used == 0.0 {
        0.0
    } else {
        research_engine::quote_for_base(levels, already_used)?
    };
    let after = research_engine::quote_for_base(levels, already_used + quantity)?;
    let value = after - before;
    (value.is_finite() && value > 0.0).then_some(value)
}

pub fn simulate(request: &PaperRequest) -> Result<PaperResult, String> {
    if !request.initial.valid() {
        return Err("initial inventory must be finite and nonnegative".into());
    }
    let replay = research_engine::replay(&ReplayRequest {
        frames: request.frames.iter().map(|f| f.evidence.clone()).collect(),
    })?;
    let first = &request.frames[0].evidence;
    let mut inventory = request.initial.clone();
    let mut exposure = 0.0;
    let mut used = HashMap::<String, LiquidityUse>::new();
    let mut entries = Vec::new();
    for (frame, decision) in request.frames.iter().zip(replay.decisions) {
        let evidence = &frame.evidence;
        if evidence.buy.instrument != first.buy.instrument
            || evidence.sell.instrument != first.sell.instrument
        {
            return Err(
                "paper run requires a fixed instrument pair; use separate ledgers for other pairs"
                    .into(),
            );
        }
        for fraction in [frame.buy_fill_fraction, frame.sell_fill_fraction] {
            if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
                return Err("fill fractions must be finite within 0..1".into());
            }
        }
        let point = decision
            .points
            .get(frame.size_index)
            .ok_or("size_index outside cost curve")?;
        let mut entry = LedgerEntry {
            as_of_ms: evidence.as_of_ms,
            buy_observation_id: evidence.buy.observation_id.clone(),
            sell_observation_id: evidence.sell.observation_id.clone(),
            buy_filled_base: 0.0,
            sell_filled_base: 0.0,
            fees_quote: 0.0,
            other_cost_quote: 0.0,
            cash_change_quote: 0.0,
            residual_base_change: exposure,
            inventory: inventory.clone(),
            status: "reference_only",
            reasons: point.reasons.clone(),
        };
        if point.conditional_net_quote.is_none() {
            entries.push(entry);
            continue;
        }
        let buy_key = register_evidence(&mut used, &evidence.buy, "ask")?;
        let sell_key = register_evidence(&mut used, &evidence.sell, "bid")?;
        let buy_qty = point.base_quantity * frame.buy_fill_fraction;
        let sell_qty = point.base_quantity * frame.sell_fill_fraction;
        if buy_qty == 0.0 && sell_qty == 0.0 {
            entry.status = "no_fill";
            entries.push(entry);
            continue;
        }
        let buy_quote = remaining_quote(&evidence.buy.asks, used[&buy_key].consumed_base, buy_qty);
        let sell_quote =
            remaining_quote(&evidence.sell.bids, used[&sell_key].consumed_base, sell_qty);
        let Some((buy_quote, sell_quote)) = buy_quote.zip(sell_quote) else {
            entry.status = "insufficient_remaining_liquidity";
            entry
                .reasons
                .push("previous frames already consumed this observed liquidity".into());
            entries.push(entry);
            continue;
        };
        let buy_fee = buy_quote * (evidence.costs.buy_fee_bps.ok_or("unknown buy fee")? / 10000.0);
        let sell_fee =
            sell_quote * (evidence.costs.sell_fee_bps.ok_or("unknown sell fee")? / 10000.0);
        let other = evidence
            .costs
            .other_cost_quote
            .ok_or("unknown other costs")?;
        let debit = buy_quote + buy_fee + other;
        if !debit.is_finite()
            || debit > inventory.buy_venue_quote
            || sell_qty > inventory.sell_venue_base
        {
            entry.status = "insufficient_inventory";
            entry
                .reasons
                .push("prefunded inventory insufficient; no borrowing inferred".into());
            entries.push(entry);
            continue;
        }
        let cash_before = inventory.cash();
        inventory.buy_venue_quote -= debit;
        inventory.buy_venue_base += buy_qty;
        inventory.sell_venue_base -= sell_qty;
        inventory.sell_venue_quote += sell_quote - sell_fee;
        exposure += buy_qty - sell_qty;
        if !inventory.valid() || !exposure.is_finite() {
            return Err("paper arithmetic overflow".into());
        }
        used.get_mut(&buy_key).expect("registered").consumed_base += buy_qty;
        used.get_mut(&sell_key).expect("registered").consumed_base += sell_qty;
        entry.buy_filled_base = buy_qty;
        entry.sell_filled_base = sell_qty;
        entry.fees_quote = buy_fee + sell_fee;
        entry.other_cost_quote = other;
        entry.cash_change_quote = inventory.cash() - cash_before;
        entry.residual_base_change = exposure;
        entry.inventory = inventory.clone();
        entry.status = if frame.buy_fill_fraction == 1.0 && frame.sell_fill_fraction == 1.0 {
            "paired_fill_scenario"
        } else {
            "partial_fill_scenario"
        };
        entries.push(entry);
    }
    let cash_change = inventory.cash() - request.initial.cash();
    Ok(PaperResult {
        model_version: "prefunded-taker-scenario/v1",
        quote_asset_id: first.buy.instrument.quote_asset_id.clone(),
        base_asset_id: first.buy.instrument.base_asset_id.clone(),
        initial: request.initial.clone(),
        final_inventory: inventory,
        net_cash_change_quote: cash_change,
        residual_base_change: exposure,
        closed_base_cash_pnl_quote: (exposure == 0.0).then_some(cash_change),
        entries,
        limitations: vec![
            "fill fractions are explicit scenarios, not predicted or guaranteed fills",
            "no borrowing, rebalancing, maker queues or automatic exit model",
            "other_cost_quote charged once per filled frame to buy-venue quote balance",
            "open base exposure is not valued; cash change is not open-position PnL",
            "book identity supplied by caller; reused IDs cannot replenish liquidity",
            "new observation IDs reset available depth; no counterfactual market-impact feedback",
            "floating-point research ledger, not settlement accounting",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PaperRequest {
        PaperRequest {
            initial: Inventory {
                buy_venue_quote: 1_000_000.0,
                buy_venue_base: 0.0,
                sell_venue_quote: 0.0,
                sell_venue_base: 10.0,
            },
            frames: vec![PaperFrame {
                evidence: crate::research_engine::tests::fixture(),
                size_index: 0,
                buy_fill_fraction: 1.0,
                sell_fill_fraction: 1.0,
            }],
        }
    }
    #[test]
    fn paired_fills_conserve_base_and_charge_both_fees() {
        let result = simulate(&fixture()).unwrap();
        assert_eq!(result.residual_base_change, 0.0);
        assert!((result.closed_base_cash_pnl_quote.unwrap() + 47.0085).abs() < 1e-8);
        assert_eq!(result.final_inventory.buy_venue_base, 1.0);
        assert_eq!(result.final_inventory.sell_venue_base, 9.0);
    }
    #[test]
    fn unmatched_leg_is_exposure_not_claimed_profit() {
        let mut request = fixture();
        request.frames[0].sell_fill_fraction = 0.0;
        let result = simulate(&request).unwrap();
        assert_eq!(result.residual_base_change, 1.0);
        assert!(result.closed_base_cash_pnl_quote.is_none());
        assert_eq!(result.entries[0].status, "partial_fill_scenario");
    }
    #[test]
    fn reused_observation_cannot_supply_infinite_liquidity() {
        let mut request = fixture();
        request.frames = vec![request.frames[0].clone(); 3];
        let result = simulate(&request).unwrap();
        assert_eq!(result.entries[2].status, "insufficient_remaining_liquidity");
        assert_eq!(result.final_inventory.buy_venue_base, 2.0);
    }
    #[test]
    fn same_observation_id_with_different_content_is_rejected() {
        let mut request = fixture();
        request.frames.push(request.frames[0].clone());
        request.frames[1].evidence.buy.asks[0].qty = 5.0;
        assert!(simulate(&request).is_err());
    }
    #[test]
    fn insufficient_inventory_and_unknown_costs_never_create_fills() {
        let mut request = fixture();
        request.initial.buy_venue_quote = 1.0;
        assert_eq!(
            simulate(&request).unwrap().entries[0].status,
            "insufficient_inventory"
        );
        request.frames[0].evidence.costs.buy_fee_bps = None;
        assert_eq!(
            simulate(&request).unwrap().entries[0].status,
            "reference_only"
        );
    }
}
