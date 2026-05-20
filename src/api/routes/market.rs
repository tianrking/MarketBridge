use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::api::ApiState;
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};
use crate::core::schema::ProductType;
#[derive(Debug, Deserialize, Default)]
pub struct MarketQuotesQuery {
    symbols: Option<String>,
    exchanges: Option<String>,
    product_type: Option<String>,
    include_stale: Option<bool>,
}

pub async fn v1_market_quotes(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MarketQuotesQuery>,
) -> impl IntoResponse {
    let symbols = q.symbols.map(parse_csv_set_upper);
    let exchanges = q.exchanges.map(parse_csv_set_lower);
    let product_type = q.product_type.map(|x| x.trim().to_ascii_lowercase());
    let include_stale = q.include_stale.unwrap_or(false);

    let mut quotes = state
        .bus
        .quote_snapshot_all()
        .await
        .into_iter()
        .filter(|quote| include_stale || !quote.freshness.stale)
        .filter(|quote| {
            symbols.as_ref().is_none_or(|set| {
                quote
                    .instrument_ref
                    .symbol
                    .as_deref()
                    .is_some_and(|symbol| set.contains(&symbol.to_ascii_uppercase()))
            })
        })
        .filter(|quote| {
            exchanges
                .as_ref()
                .is_none_or(|set| set.contains(&quote.source_ref.source.to_ascii_lowercase()))
        })
        .filter(|quote| {
            product_type.as_ref().is_none_or(|value| {
                product_type_label(quote.instrument_ref.product_type).eq_ignore_ascii_case(value)
            })
        })
        .collect::<Vec<_>>();

    quotes.sort_by(|a, b| {
        a.instrument_ref
            .instrument_id
            .cmp(&b.instrument_ref.instrument_id)
            .then_with(|| a.source_ref.source.cmp(&b.source_ref.source))
    });

    Json(serde_json::json!({
        "version": "v1",
        "domain": "market_quote",
        "quotes": quotes
    }))
}

fn product_type_label(product_type: ProductType) -> &'static str {
    match product_type {
        ProductType::Spot => "spot",
        ProductType::Perp => "perp",
        ProductType::Future => "future",
        ProductType::Option => "option",
        ProductType::BinaryOutcome => "binary_outcome",
        ProductType::WalletTransfer => "wallet_transfer",
        ProductType::DexPool => "dex_pool",
        ProductType::Event => "event",
    }
}
