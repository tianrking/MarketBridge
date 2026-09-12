//! Streaming replay of an entire sealed normalized journal for one explicit route.
use crate::{
    api::routes::opportunities::LiveScanRequest,
    domains::market::quote::QuoteKind,
    research_engine::{BookEvidence, ScanRequest},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub fn replay(path: &std::path::Path, route: LiveScanRequest) -> Result<Value> {
    route.buy.validate().map_err(anyhow::Error::msg)?;
    route.sell.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        route.buy.id != route.sell.id,
        "different route legs required"
    );
    let mut buy = None;
    let mut sell = None;
    let mut previous = 0;
    let mut decisions = 0u64;
    let mut positives = 0u64;
    let mut reference = 0u64;
    let mut max_net_bps: Option<f64> = None;
    let mut first_time = None;
    let mut last_time = None;
    let mut last_result = None;
    let verification = crate::journal::visit(path, |record| {
        if record.payload["type"] != "order_book" {
            return Ok(());
        }
        let p = &record.payload;
        if p["market"] != "spot" {
            return Ok(());
        }
        let instrument = if p["exchange"] == route.buy.venue && p["symbol"] == route.buy.symbol {
            &route.buy
        } else if p["exchange"] == route.sell.venue && p["symbol"] == route.sell.symbol {
            &route.sell
        } else {
            return Ok(());
        };
        ensure!(
            record.received_at_ms >= previous && record.received_at_ms > 0,
            "journal observation clock moved backwards"
        );
        previous = record.received_at_ms;
        let complete = matches!(instrument.venue.as_str(), "binance" | "okx");
        let book = BookEvidence {
            observation_id: format!("journal:{}", record.sequence),
            instrument: instrument.clone(),
            quote_kind: if complete {
                QuoteKind::ObservedBook
            } else {
                QuoteKind::Reference
            },
            source_time_ms: p["ts_ms"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("book source time missing"))?,
            received_at_ms: record.received_at_ms,
            complete,
            bids: serde_json::from_value(p["bids"].clone())?,
            asks: serde_json::from_value(p["asks"].clone())?,
        };
        if instrument.id == route.buy.id {
            buy = Some(book);
        } else {
            sell = Some(book);
        }
        if let Some((buy, sell)) = buy.as_ref().zip(sell.as_ref()) {
            let input = ScanRequest {
                as_of_ms: record.received_at_ms,
                max_age_ms: route.max_age_ms,
                max_skew_ms: route.max_skew_ms,
                relationship: route.relationship.clone(),
                buy: buy.clone(),
                sell: sell.clone(),
                quantities: route.quantities.clone(),
                costs: route.costs.clone(),
            };
            let result = crate::research_engine::scan(&input).map_err(anyhow::Error::msg)?;
            decisions += 1;
            first_time.get_or_insert(input.as_of_ms);
            last_time = Some(input.as_of_ms);
            if result
                .points
                .iter()
                .any(|p| p.conditional_net_quote.is_some_and(|n| n > 0.0))
            {
                positives += 1;
            }
            if result
                .points
                .iter()
                .all(|p| p.conditional_net_quote.is_none())
            {
                reference += 1;
            }
            for point in &result.points {
                if let Some(bps) = point.net_bps_on_buy_notional {
                    max_net_bps = Some(max_net_bps.map_or(bps, |old| old.max(bps)));
                }
            }
            last_result = Some(result);
        }
        Ok(())
    })?;
    ensure!(
        verification.sealed,
        "full replay requires a sealed journal, not a crash prefix"
    );
    ensure!(
        verification.source_dropped_total == 0,
        "recorded drop counter is nonzero; incomplete history cannot pass strict full replay"
    );
    ensure!(decisions > 0, "no matching two-sided books for route");
    Ok(
        json!({"model_version":"normalized-journal-replay/v1","verification":verification,"route":route,"decision_count":decisions,"positive_conditional_observations":positives,"reference_only_observations":reference,"max_net_bps":max_net_bps,"first_decision_ms":first_time,"last_decision_ms":last_time,"last_result":last_result,"orders_supported":false,"limitations":["replayed every matching recorded book in file order with receipt-time decisions","normalized journal, not exchange raw messages or proof of upstream completeness","positive observations and sizes overlap and must not be summed as profit","caller-attested identity/costs; historical adapter schema/version must match","report only: no portfolio execution, latency simulation or future knowledge repair"]}),
    )
}
