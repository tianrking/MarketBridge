use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::Value;

use crate::api::ApiState;
use crate::api::error::upstream_error;
use crate::onchain::OnchainTransferQuery;

#[derive(Debug, Deserialize, Default)]
pub struct OnchainTransfersHttpQuery {
    source: Option<String>,
    chain: Option<String>,
    asset: Option<String>,
    min_amount_usd: Option<f64>,
    limit: Option<usize>,
}

pub async fn v1_onchain_transfers(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<OnchainTransfersHttpQuery>,
) -> impl IntoResponse {
    let rows = state
        .onchain_store
        .query(OnchainTransferQuery {
            source: q.source.map(|x| x.trim().to_ascii_lowercase()),
            chain: q.chain.map(|x| x.trim().to_ascii_lowercase()),
            asset: q.asset.map(|x| x.trim().to_ascii_uppercase()),
            min_amount_usd: q.min_amount_usd,
            limit: q.limit.unwrap_or(500),
        })
        .await;
    Json(serde_json::json!({
        "version": "v1",
        "domain": "onchain_transfer",
        "transfers": rows
    }))
}

/// Return a bounded, read-only Bitcoin mempool and fee-pressure snapshot.
///
/// This deliberately uses provider aggregates rather than attempting to expose
/// addresses, transaction broadcasts, or wallet actions through MarketBridge.
pub async fn v1_onchain_mempool(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mempool = match state
        .http
        .get("https://mempool.space/api/mempool")
        .send()
        .await
        .and_then(|response| response.error_for_status())
    {
        Ok(response) => match response.json::<Value>().await {
            Ok(payload) => payload,
            Err(error) => return upstream_error("mempool_space", error),
        },
        Err(error) => return upstream_error("mempool_space", error),
    };
    let fees = match state
        .http
        .get("https://mempool.space/api/v1/fees/recommended")
        .send()
        .await
        .and_then(|response| response.error_for_status())
    {
        Ok(response) => match response.json::<Value>().await {
            Ok(payload) => payload,
            Err(error) => return upstream_error("mempool_space", error),
        },
        Err(error) => return upstream_error("mempool_space", error),
    };
    let tip_height = match state
        .http
        .get("https://mempool.space/api/blocks/tip/height")
        .send()
        .await
        .and_then(|response| response.error_for_status())
    {
        Ok(response) => match response.text().await {
            Ok(value) => value,
            Err(error) => return upstream_error("mempool_space", error),
        },
        Err(error) => return upstream_error("mempool_space", error),
    };

    match normalize_mempool_context(&mempool, &fees, &tip_height) {
        Some(data) => Json(serde_json::json!({
            "version": "v1",
            "domain": "bitcoin_mempool_context",
            "source": "mempool_space",
            "coverage": "provider_snapshot",
            "data": data,
            "limitations": [
                "mempool state is provider- and node-dependent, not a complete network-wide ledger",
                "recommended fee rates are guidance and do not guarantee confirmation timing",
                "fee pressure is network context, not a directional BTC forecast or transaction instruction",
                "this read-only endpoint does not broadcast transactions, sign wallets, or execute trades"
            ]
        }))
        .into_response(),
        None => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "version": "v1",
                "domain": "bitcoin_mempool_context",
                "source": "mempool_space",
                "error": "mempool.space payload missing required aggregate fields"
            })),
        )
            .into_response(),
    }
}

fn normalize_mempool_context(mempool: &Value, fees: &Value, tip_height: &str) -> Option<Value> {
    let count = mempool.get("count").and_then(Value::as_u64)?;
    let vsize = mempool.get("vsize").and_then(Value::as_u64)?;
    let total_fee_sats = mempool.get("total_fee").and_then(Value::as_u64)?;
    let tip_height = tip_height.trim().parse::<u64>().ok()?;
    let fastest_fee = fees.get("fastestFee").and_then(Value::as_u64)?;
    let half_hour_fee = fees.get("halfHourFee").and_then(Value::as_u64)?;
    let hour_fee = fees.get("hourFee").and_then(Value::as_u64)?;
    let economy_fee = fees.get("economyFee").and_then(Value::as_u64)?;
    let minimum_fee = fees.get("minimumFee").and_then(Value::as_u64)?;

    Some(serde_json::json!({
        "mempool_count": count,
        "mempool_vsize": vsize,
        "mempool_vsize_mb": vsize as f64 / 1_000_000.0,
        "mempool_total_fee_btc": total_fee_sats as f64 / 100_000_000.0,
        "fee_rates_sat_vb": {
            "fastest": fastest_fee,
            "half_hour": half_hour_fee,
            "hour": hour_fee,
            "economy": economy_fee,
            "minimum": minimum_fee
        },
        "tip_height": tip_height
    }))
}

#[cfg(test)]
mod tests {
    use super::normalize_mempool_context;
    use serde_json::json;

    #[test]
    fn normalizes_mempool_and_recommended_fee_payloads() {
        let data = normalize_mempool_context(
            &json!({"count": 12, "vsize": 345678, "total_fee": 12345678}),
            &json!({"fastestFee": 24, "halfHourFee": 18, "hourFee": 12, "economyFee": 5, "minimumFee": 1}),
            "900000\n",
        )
        .expect("complete provider payload should normalize");
        assert_eq!(data["mempool_count"], 12);
        assert_eq!(data["mempool_vsize"], 345678);
        assert_eq!(data["fee_rates_sat_vb"]["fastest"], 24);
        assert_eq!(data["tip_height"], 900000);
        assert!((data["mempool_total_fee_btc"].as_f64().unwrap() - 0.12345678).abs() < 1e-9);
    }

    #[test]
    fn rejects_incomplete_provider_payload() {
        assert!(
            normalize_mempool_context(&json!({"count": 12}), &json!({"fastestFee": 24}), "900000",)
                .is_none()
        );
    }
}
