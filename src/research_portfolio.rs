//! Allocated spot portfolios, reversible routes and explicit exit conditions.
//! No margin, shorting, currency conversion or guaranteed stop execution.
use crate::{
    core::instrument::Instrument,
    paper::{LiquidityUse, register_evidence, remaining_quote},
    research_engine::{ScanRequest, scan},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub instrument: Instrument,
    pub base: f64,
    pub quote: f64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Gate {
    Always,
    NetBpsAtLeast { bps: f64 },
    TimeAtOrAfter { time_ms: u64 },
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub purpose: Purpose,
    pub gate: Gate,
    pub evidence: ScanRequest,
    pub size_index: usize,
    pub buy_fill_fraction: f64,
    pub sell_fill_fraction: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Entry,
    Exit,
    Rebalance,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Portfolio {
    pub accounts: Vec<Account>,
    pub steps: Vec<Step>,
}
fn totals(accounts: &BTreeMap<String, Account>) -> (f64, BTreeMap<String, f64>) {
    let mut bases = BTreeMap::new();
    let mut cash = 0.0;
    for a in accounts.values() {
        cash += a.quote;
        *bases.entry(a.instrument.base_asset_id.clone()).or_default() += a.base;
    }
    (cash, bases)
}
pub fn simulate(input: &Portfolio) -> Result<Value> {
    ensure!(
        (2..=128).contains(&input.accounts.len()) && (1..=512).contains(&input.steps.len()),
        "portfolio requires 2..128 accounts and 1..512 steps"
    );
    let mut accounts = BTreeMap::new();
    let mut identities = HashSet::new();
    let common = &input.accounts[0].instrument;
    let mut base_identities: HashMap<&str, &Instrument> = HashMap::new();
    for a in &input.accounts {
        a.instrument.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            a.instrument.product_type == crate::core::schema::ProductType::Spot
                && a.instrument.contract_multiplier == 1.0,
            "allocated portfolio supports unit-multiplier spot instruments only"
        );
        ensure!(
            a.base.is_finite() && a.quote.is_finite() && a.base >= 0.0 && a.quote >= 0.0,
            "invalid allocated inventory"
        );
        ensure!(
            a.instrument.quote_asset_id == common.quote_asset_id
                && a.instrument.price_unit == common.price_unit,
            "portfolio requires one quote currency/unit; no implicit FX"
        );
        if let Some(previous) = base_identities.insert(&a.instrument.base_asset_id, &a.instrument) {
            ensure!(
                previous.same_spot_asset(&a.instrument),
                "same base ID has incompatible units or asset identity"
            );
        }
        ensure!(
            identities.insert((&a.instrument.venue, &a.instrument.symbol))
                && accounts
                    .insert(a.instrument.id.clone(), a.clone())
                    .is_none(),
            "duplicate account/instrument allocation"
        );
    }
    let (initial_cash, initial_base) = totals(&accounts);
    ensure!(
        initial_cash.is_finite() && initial_base.values().all(|v| v.is_finite()),
        "initial totals overflow"
    );
    let mut used = HashMap::<String, LiquidityUse>::new();
    let mut entries = Vec::new();
    let mut last = 0;
    let mut ids = HashSet::new();
    for step in &input.steps {
        ensure!(
            crate::research_store::valid_id(&step.id) && ids.insert(&step.id),
            "invalid/duplicate step ID"
        );
        let e = &step.evidence;
        ensure!(e.as_of_ms >= last, "steps must be chronological");
        last = e.as_of_ms;
        for f in [step.buy_fill_fraction, step.sell_fill_fraction] {
            ensure!(
                f.is_finite() && (0.0..=1.0).contains(&f),
                "invalid fill fraction"
            );
        }
        let buy = accounts
            .get(&e.buy.instrument.id)
            .ok_or_else(|| anyhow::anyhow!("buy account missing"))?
            .clone();
        let sell = accounts
            .get(&e.sell.instrument.id)
            .ok_or_else(|| anyhow::anyhow!("sell account missing"))?
            .clone();
        ensure!(
            buy.instrument == e.buy.instrument
                && sell.instrument == e.sell.instrument
                && buy.instrument.id != sell.instrument.id,
            "account evidence identity mismatch"
        );
        let decision = scan(e).map_err(anyhow::Error::msg)?;
        let point = decision
            .points
            .get(step.size_index)
            .ok_or_else(|| anyhow::anyhow!("invalid size index"))?;
        let gate = match step.gate {
            Gate::Always => true,
            Gate::NetBpsAtLeast { bps } => {
                ensure!(bps.is_finite(), "invalid threshold");
                point.net_bps_on_buy_notional.is_some_and(|n| n >= bps)
            }
            Gate::TimeAtOrAfter { time_ms } => {
                ensure!(time_ms > 0, "invalid time gate");
                e.as_of_ms >= time_ms
            }
        };
        let mut status = "reference_only";
        let mut fills = json!(null);
        if !gate {
            status = "condition_not_met";
        } else if point.conditional_net_quote.is_some() {
            let bk = register_evidence(&mut used, &e.buy, "ask").map_err(anyhow::Error::msg)?;
            let sk = register_evidence(&mut used, &e.sell, "bid").map_err(anyhow::Error::msg)?;
            let bq = point.base_quantity * step.buy_fill_fraction;
            let sq = point.base_quantity * step.sell_fill_fraction;
            if bq == 0.0 && sq == 0.0 {
                status = "no_fill";
            } else if let Some((bc, sc)) = remaining_quote(&e.buy.asks, used[&bk].consumed_base, bq)
                .zip(remaining_quote(&e.sell.bids, used[&sk].consumed_base, sq))
            {
                let bf = bc * e.costs.buy_fee_bps.unwrap() / 10000.0;
                let sf = sc * e.costs.sell_fee_bps.unwrap() / 10000.0;
                let other = e.costs.other_cost_quote.unwrap();
                let debit = bc + bf + other;
                let credit = sc - sf;
                let (_, current_base) = totals(&accounts);
                let asset = &buy.instrument.base_asset_id;
                let old_exposure = current_base[asset] - initial_base[asset];
                let new_exposure = old_exposure + bq - sq;
                if matches!(step.purpose, Purpose::Exit) && new_exposure.abs() > old_exposure.abs()
                {
                    status = "exit_would_increase_exposure";
                } else if debit > buy.quote || sq > sell.base {
                    status = "insufficient_inventory";
                } else {
                    let next_buy = Account {
                        base: buy.base + bq,
                        quote: buy.quote - debit,
                        ..buy.clone()
                    };
                    let next_sell = Account {
                        base: sell.base - sq,
                        quote: sell.quote + credit,
                        ..sell.clone()
                    };
                    ensure!(
                        [
                            next_buy.base,
                            next_buy.quote,
                            next_sell.base,
                            next_sell.quote,
                            new_exposure
                        ]
                        .into_iter()
                        .all(f64::is_finite)
                            && next_buy.quote >= 0.0
                            && next_sell.quote >= 0.0,
                        "portfolio arithmetic invalid"
                    );
                    accounts.insert(buy.instrument.id.clone(), next_buy);
                    accounts.insert(sell.instrument.id.clone(), next_sell);
                    used.get_mut(&bk).unwrap().consumed_base += bq;
                    used.get_mut(&sk).unwrap().consumed_base += sq;
                    status = "fill_scenario";
                    fills = json!({"buy_base":bq,"sell_base":sq,"fees_quote":bf+sf,"other_cost_quote":other,"cash_change_quote":credit-debit});
                }
            } else {
                status = "insufficient_remaining_liquidity";
            }
        }
        entries.push(json!({"id":step.id,"purpose":step.purpose,"as_of_ms":e.as_of_ms,"status":status,"fills":fills,"reasons":point.reasons,"buy_observation_id":e.buy.observation_id,"sell_observation_id":e.sell.observation_id}));
    }
    let (cash, bases) = totals(&accounts);
    let cash_change = cash - initial_cash;
    let exposure: BTreeMap<_, _> = bases
        .iter()
        .map(|(id, b)| (id.clone(), b - initial_base[id]))
        .collect();
    ensure!(
        cash_change.is_finite() && exposure.values().all(|v| v.is_finite()),
        "portfolio totals overflow"
    );
    Ok(
        json!({"model_version":"allocated-spot-portfolio/v1","quote_asset_id":common.quote_asset_id,"accounts":accounts,"entries":entries,"net_cash_change_quote":cash_change,"residual_base_changes":exposure,"closed_base_cash_pnl_quote":if exposure.values().all(|v|*v==0.0){Some(cash_change)}else{None},"orders_supported":false,"limitations":["quote balances are explicitly allocated per instrument, not duplicated shared balances","supplied fill fractions and observed depth do not predict execution","exit requires observable eligible evidence, inventory and depth; stop prices are not guaranteed","no borrowing, margin, settlement, automatic FX or valuation of open exposure","new observation IDs replenish scenario liquidity; no counterfactual impact model"]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversed_route_closes_partial_entry_without_inventing_pnl() {
        let e = crate::research_engine::tests::fixture();
        let a = Account {
            instrument: e.buy.instrument.clone(),
            base: 0.0,
            quote: 1e6,
        };
        let b = Account {
            instrument: e.sell.instrument.clone(),
            base: 10.0,
            quote: 1e6,
        };
        let entry = Step {
            id: "entry".into(),
            purpose: Purpose::Entry,
            gate: Gate::Always,
            evidence: e.clone(),
            size_index: 0,
            buy_fill_fraction: 1.0,
            sell_fill_fraction: 0.0,
        };
        let mut exit = entry.clone();
        exit.id = "exit".into();
        exit.purpose = Purpose::Exit;
        exit.buy_fill_fraction = 0.0;
        exit.sell_fill_fraction = 1.0;
        std::mem::swap(&mut exit.evidence.buy, &mut exit.evidence.sell);
        std::mem::swap(
            &mut exit.evidence.relationship.left_instrument,
            &mut exit.evidence.relationship.right_instrument,
        );
        let partial = simulate(&Portfolio {
            accounts: vec![a.clone(), b.clone()],
            steps: vec![entry.clone()],
        })
        .unwrap();
        assert!(partial["closed_base_cash_pnl_quote"].is_null());
        let closed = simulate(&Portfolio {
            accounts: vec![a, b],
            steps: vec![entry, exit],
        })
        .unwrap();
        assert!(closed["closed_base_cash_pnl_quote"].as_f64().unwrap() < 0.0);
        assert_eq!(closed["entries"][1]["status"], "fill_scenario");
    }
}
