//! Pure, bounded, deterministic research functions. No network or order APIs.
use serde::{Deserialize, Serialize};

use crate::core::instrument::{AssetRelationship, Instrument, RelationshipKind};
use crate::domains::market::quote::QuoteKind;
use crate::types::{BookLevel, timestamp_is_fresh};

pub const MODEL_VERSION: &str = "same-asset-spot/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookEvidence {
    pub observation_id: String,
    pub instrument: Instrument,
    pub quote_kind: QuoteKind,
    pub source_time_ms: u64,
    pub received_at_ms: u64,
    pub complete: bool,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostAssumptions {
    pub version: String,
    pub buy_fee_bps: Option<f64>,
    pub sell_fee_bps: Option<f64>,
    /// Additional total cost in the common quote currency at EACH requested size.
    /// Explicit zero means the scenario excludes funding/rebalancing/etc.
    pub other_cost_quote: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    pub as_of_ms: u64,
    pub max_age_ms: u64,
    pub max_skew_ms: u64,
    pub relationship: AssetRelationship,
    pub buy: BookEvidence,
    pub sell: BookEvidence,
    pub quantities: Vec<f64>,
    pub costs: CostAssumptions,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurvePoint {
    pub base_quantity: f64,
    pub buy_quote: Option<f64>,
    pub sell_quote: Option<f64>,
    pub gross_quote: Option<f64>,
    pub buy_fee_quote: Option<f64>,
    pub sell_fee_quote: Option<f64>,
    pub other_cost_quote: Option<f64>,
    pub conditional_net_quote: Option<f64>,
    pub net_bps_on_buy_notional: Option<f64>,
    pub status: &'static str,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub model_version: &'static str,
    pub as_of_ms: u64,
    pub buy_observation_id: String,
    pub sell_observation_id: String,
    pub cost_version: String,
    pub quote_asset_id: String,
    pub quote_time_skew_ms: u64,
    pub points: Vec<CurvePoint>,
    pub limitations: Vec<&'static str>,
}

fn valid_levels(levels: &[BookLevel], ascending: bool) -> bool {
    !levels.is_empty()
        && levels.len() <= 200
        && levels.iter().all(|l| {
            l.price.is_finite()
                && l.price > 0.0
                && l.qty.is_finite()
                && l.qty > 0.0
                && (l.price * l.qty).is_finite()
        })
        && levels.windows(2).all(|w| {
            if ascending {
                w[0].price < w[1].price
            } else {
                w[0].price > w[1].price
            }
        })
}

pub fn validate_book(book: &BookEvidence) -> Result<(), String> {
    book.instrument.validate()?;
    if book.observation_id.trim().is_empty() || book.observation_id.len() > 256 {
        return Err("observation_id must contain 1..256 bytes".into());
    }
    if !valid_levels(&book.bids, false) || !valid_levels(&book.asks, true) {
        return Err("book requires 1..200 finite positive strictly sorted levels per side".into());
    }
    if book.bids[0].price > book.asks[0].price {
        return Err("locally crossed book is not a valid continuous-market snapshot".into());
    }
    Ok(())
}

/// Integrate exactly the same base quantity on each leg. Price impact is already
/// inside this amount; do not subtract that same impact a second time.
pub fn quote_for_base(levels: &[BookLevel], quantity: f64) -> Option<f64> {
    if !quantity.is_finite() || quantity <= 0.0 {
        return None;
    }
    let mut remaining = quantity;
    let mut quote = 0.0;
    for level in levels {
        if !level.price.is_finite()
            || !level.qty.is_finite()
            || level.price <= 0.0
            || level.qty <= 0.0
        {
            return None;
        }
        let take = remaining.min(level.qty);
        quote += take * level.price;
        remaining -= take;
        if remaining <= quantity * 1e-12 {
            break;
        }
    }
    (remaining <= quantity * 1e-12 && quote.is_finite()).then_some(quote)
}

pub fn scan(request: &ScanRequest) -> Result<ScanResult, String> {
    validate_book(&request.buy)?;
    validate_book(&request.sell)?;
    if request.as_of_ms == 0
        || request.max_age_ms == 0
        || request.quantities.is_empty()
        || request.quantities.len() > 32
        || request
            .quantities
            .iter()
            .any(|q| !q.is_finite() || *q <= 0.0)
    {
        return Err("positive as_of/max_age and 1..32 positive finite quantities required".into());
    }
    if request.costs.version.trim().is_empty() || request.costs.version.len() > 256 {
        return Err("cost version required (max 256 bytes)".into());
    }
    for fee in [request.costs.buy_fee_bps, request.costs.sell_fee_bps]
        .into_iter()
        .flatten()
    {
        if !fee.is_finite() || !(0.0..=10_000.0).contains(&fee) {
            return Err("taker fees must be finite within 0..10000 bps".into());
        }
    }
    if request
        .costs
        .other_cost_quote
        .is_some_and(|v| !v.is_finite() || v < 0.0)
    {
        return Err("other_cost_quote must be finite and nonnegative".into());
    }
    let mut reasons = Vec::new();
    let relation = &request.relationship;
    let comparable = relation.kind == RelationshipKind::SameAsset
        && relation.left_instrument == request.buy.instrument.id
        && relation.right_instrument == request.sell.instrument.id
        && !relation.evidence.trim().is_empty()
        && relation.known_at_ms > 0
        && relation.known_at_ms <= request.as_of_ms
        && relation
            .valid_until_ms
            .is_none_or(|t| t >= request.as_of_ms)
        && request
            .buy
            .instrument
            .same_spot_asset(&request.sell.instrument)
        && request.buy.instrument.contract_multiplier == 1.0;
    if !comparable {
        reasons.push("relationship_not_supported_or_unverified".into());
    }
    if request.buy.instrument.venue == request.sell.instrument.venue {
        reasons.push("same_venue".into());
    }
    for (leg, book) in [("buy", &request.buy), ("sell", &request.sell)] {
        if book.received_at_ms == 0 || book.received_at_ms > request.as_of_ms {
            reasons.push(format!("{leg}_not_known_at_decision"));
        }
        if !timestamp_is_fresh(book.source_time_ms, request.as_of_ms, request.max_age_ms)
            || request.as_of_ms.saturating_sub(book.received_at_ms) > request.max_age_ms
            || book.source_time_ms > book.received_at_ms.saturating_add(1_000)
        {
            reasons.push(format!("{leg}_stale_or_clock_invalid"));
        }
        if book.quote_kind != QuoteKind::ObservedBook || !book.complete {
            reasons.push(format!("{leg}_not_complete_observed_book"));
        }
    }
    let skew = request
        .buy
        .source_time_ms
        .abs_diff(request.sell.source_time_ms);
    if skew > request.max_skew_ms {
        reasons.push("quote_time_skew".into());
    }
    if request.costs.buy_fee_bps.is_none() {
        reasons.push("unknown_buy_fee".into());
    }
    if request.costs.sell_fee_bps.is_none() {
        reasons.push("unknown_sell_fee".into());
    }
    if request.costs.other_cost_quote.is_none() {
        reasons.push("unknown_other_costs".into());
    }

    let points = request
        .quantities
        .iter()
        .map(|&q| {
            let mut reasons = reasons.clone();
            let buy = comparable
                .then(|| quote_for_base(&request.buy.asks, q))
                .flatten();
            let sell = comparable
                .then(|| quote_for_base(&request.sell.bids, q))
                .flatten();
            if comparable && (buy.is_none() || sell.is_none()) {
                reasons.push("insufficient_depth".into());
            }
            let gross = buy.zip(sell).map(|(b, s)| s - b);
            let buy_fee = buy
                .zip(request.costs.buy_fee_bps)
                .map(|(n, f)| n * (f / 10_000.0));
            let sell_fee = sell
                .zip(request.costs.sell_fee_bps)
                .map(|(n, f)| n * (f / 10_000.0));
            let net = if reasons.is_empty() {
                gross
                    .zip(buy_fee)
                    .zip(sell_fee)
                    .zip(request.costs.other_cost_quote)
                    .map(|(((g, b), s), o)| g - b - s - o)
            } else {
                None
            };
            let net_bps = net.zip(buy).map(|(n, b)| n / b * 10_000.0);
            if net.is_some_and(|n| !n.is_finite()) || net_bps.is_some_and(|n| !n.is_finite()) {
                reasons.push("numeric_overflow".into());
            }
            let valid = reasons.is_empty();
            CurvePoint {
                base_quantity: q,
                buy_quote: buy,
                sell_quote: sell,
                gross_quote: gross,
                buy_fee_quote: buy_fee,
                sell_fee_quote: sell_fee,
                other_cost_quote: request.costs.other_cost_quote,
                conditional_net_quote: valid.then_some(net).flatten(),
                net_bps_on_buy_notional: valid.then_some(net_bps).flatten(),
                status: if valid {
                    "conditional_estimate"
                } else {
                    "reference_only"
                },
                reasons,
            }
        })
        .collect();
    Ok(ScanResult {
        model_version: MODEL_VERSION,
        as_of_ms: request.as_of_ms,
        buy_observation_id: request.buy.observation_id.clone(),
        sell_observation_id: request.sell.observation_id.clone(),
        cost_version: request.costs.version.clone(),
        quote_asset_id: request.buy.instrument.quote_asset_id.clone(),
        quote_time_skew_ms: skew,
        points,
        limitations: vec![
            "caller-supplied identity and evidence; not independently attested",
            "prefunded taker scenario; no orders or simultaneous fill guarantee",
            "other_cost_quote is a per-size total assumption, not a measured universal cost",
            "floating-point research estimates; not settlement accounting",
        ],
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayRequest {
    pub frames: Vec<ScanRequest>,
}

#[derive(Debug, Serialize)]
pub struct ReplayResult {
    pub model_version: &'static str,
    pub decisions: Vec<ScanResult>,
}

pub fn replay(request: &ReplayRequest) -> Result<ReplayResult, String> {
    if request.frames.is_empty() || request.frames.len() > 512 {
        return Err("replay requires 1..512 frames".into());
    }
    if request
        .frames
        .windows(2)
        .any(|w| w[0].as_of_ms > w[1].as_of_ms)
    {
        return Err("replay decisions must be ordered by as_of_ms".into());
    }
    // Do not silently transform future information into a historical observation.
    if request.frames.iter().any(|f| {
        f.buy.received_at_ms > f.as_of_ms
            || f.sell.received_at_ms > f.as_of_ms
            || f.relationship.known_at_ms > f.as_of_ms
    }) {
        return Err("replay contains information not known at the decision time".into());
    }
    Ok(ReplayResult {
        model_version: MODEL_VERSION,
        decisions: request.frames.iter().map(scan).collect::<Result<_, _>>()?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::schema::{AssetClass, ProductType};

    #[test]
    fn generic_spot_model_accepts_explicit_commodity_identity_not_a_crypto_ticker_rule() {
        let mut f = fixture();
        for book in [&mut f.buy, &mut f.sell] {
            book.instrument.asset_class = AssetClass::Commodity;
            book.instrument.base_asset_id = "fixture:gold-spot".into();
            book.instrument.quantity_unit = "troy_ounce".into();
            book.instrument.price_unit = "USD/troy_ounce".into();
            book.instrument.symbol = "XAUUSD".into();
        }
        assert!(scan(&f).unwrap().points[0].conditional_net_quote.is_some());
        f.sell.instrument.product_type = ProductType::Future;
        f.sell.instrument.expiry_ms = Some(100_000);
        assert!(scan(&f).unwrap().points[0].conditional_net_quote.is_none());
    }

    #[test]
    fn shipped_example_is_valid_and_insufficient_size_is_explicit() {
        let request: ScanRequest =
            serde_json::from_str(include_str!("../examples/research/same-asset.json")).unwrap();
        let result = scan(&request).unwrap();
        assert_eq!(result.points.len(), 4);
        assert!(result.points[0].conditional_net_quote.unwrap() < 0.0);
        assert!(
            result.points[3]
                .reasons
                .contains(&"insufficient_depth".into())
        );
    }

    pub fn fixture() -> ScanRequest {
        let instrument = Instrument {
            id: "a:spot:BTCUSD".into(),
            venue: "a".into(),
            symbol: "BTCUSD".into(),
            asset_class: AssetClass::Crypto,
            product_type: ProductType::Spot,
            base_asset_id: "bitcoin:native".into(),
            quote_asset_id: "iso4217:USD".into(),
            quantity_unit: "BTC".into(),
            price_unit: "USD/BTC".into(),
            contract_multiplier: 1.0,
            expiry_ms: None,
            chain: None,
            contract_address: None,
            issuer: None,
            settlement: None,
        };
        let buy = BookEvidence {
            observation_id: "a:1".into(),
            instrument,
            quote_kind: QuoteKind::ObservedBook,
            source_time_ms: 10_000,
            received_at_ms: 10_010,
            complete: true,
            bids: vec![BookLevel {
                price: 60_000.0,
                qty: 2.0,
            }],
            asks: vec![BookLevel {
                price: 60_002.0,
                qty: 2.0,
            }],
        };
        let mut sell = buy.clone();
        sell.instrument.id = "b:spot:BTCUSD".into();
        sell.instrument.venue = "b".into();
        sell.observation_id = "b:1".into();
        sell.bids[0].price = 60_015.0;
        sell.asks[0].price = 60_018.0;
        ScanRequest {
            as_of_ms: 10_020,
            max_age_ms: 1_000,
            max_skew_ms: 100,
            relationship: AssetRelationship {
                left_instrument: buy.instrument.id.clone(),
                right_instrument: sell.instrument.id.clone(),
                kind: RelationshipKind::SameAsset,
                evidence: "fixture registry".into(),
                known_at_ms: 9_000,
                valid_until_ms: None,
            },
            buy,
            sell,
            quantities: vec![1.0],
            costs: CostAssumptions {
                version: "fixture/1".into(),
                buy_fee_bps: Some(5.0),
                sell_fee_bps: Some(5.0),
                other_cost_quote: Some(0.0),
            },
        }
    }

    #[test]
    fn costs_reverse_positive_gross_without_changing_base_quantity() {
        let result = scan(&fixture()).unwrap();
        let p = &result.points[0];
        assert_eq!(p.gross_quote, Some(13.0));
        assert!((p.conditional_net_quote.unwrap() + 47.0085).abs() < 1e-9);
    }

    #[test]
    fn unavailable_costs_are_not_zero_and_reference_is_not_book() {
        let mut f = fixture();
        f.costs.buy_fee_bps = None;
        let p = scan(&f).unwrap().points.remove(0);
        assert_eq!(p.gross_quote, Some(13.0));
        assert!(p.conditional_net_quote.is_none());
        f.costs.buy_fee_bps = Some(0.0);
        f.buy.quote_kind = QuoteKind::Synthetic;
        assert!(scan(&f).unwrap().points[0].conditional_net_quote.is_none());
    }

    #[test]
    fn rejects_crossed_or_unsorted_books_and_inadequate_depth() {
        let mut f = fixture();
        f.buy.bids[0].price = 70_000.0;
        assert!(scan(&f).is_err());
        let mut f = fixture();
        f.quantities = vec![3.0];
        assert!(
            scan(&f).unwrap().points[0]
                .reasons
                .contains(&"insufficient_depth".into())
        );
    }

    #[test]
    fn same_ticker_different_asset_or_chain_is_not_fungible() {
        let mut f = fixture();
        f.sell.instrument.base_asset_id = "wrapped:BTC".into();
        assert!(scan(&f).unwrap().points[0].gross_quote.is_none());
        let mut f = fixture();
        f.relationship.kind = RelationshipKind::Correlated;
        assert!(scan(&f).unwrap().points[0].conditional_net_quote.is_none());
    }

    #[test]
    fn replay_is_deterministic_and_rejects_future_information() {
        let mut r = ReplayRequest {
            frames: vec![fixture()],
        };
        assert_eq!(
            serde_json::to_value(replay(&r).unwrap()).unwrap(),
            serde_json::to_value(replay(&r).unwrap()).unwrap()
        );
        r.frames[0].sell.received_at_ms = 20_000;
        assert!(replay(&r).is_err());
    }

    #[test]
    fn stale_and_time_skew_are_explained() {
        let mut f = fixture();
        f.as_of_ms = 20_000;
        assert!(
            scan(&f).unwrap().points[0]
                .reasons
                .contains(&"buy_stale_or_clock_invalid".into())
        );
        let mut f = fixture();
        f.sell.source_time_ms -= 200;
        assert!(
            scan(&f).unwrap().points[0]
                .reasons
                .contains(&"quote_time_skew".into())
        );
    }

    #[test]
    fn multi_level_quantity_arithmetic() {
        let levels = vec![
            BookLevel {
                price: 60_002.0,
                qty: 0.5,
            },
            BookLevel {
                price: 60_004.0,
                qty: 1.2,
            },
        ];
        let buy = quote_for_base(&levels, 0.8).unwrap();
        assert!((48_012.0 - buy - 9.8).abs() < 1e-9);
    }
}
