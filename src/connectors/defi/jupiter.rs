use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::{JupiterConfig, SolanaQuotePair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_metric, emit_defi_quote, parse_f64_str, quote_to_price};

pub struct JupiterQuotePoller {
    cfg: JupiterConfig,
    client: reqwest::Client,
}

impl JupiterQuotePoller {
    pub fn new(cfg: JupiterConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JupiterQuoteResponse {
    in_amount: String,
    out_amount: String,
    #[serde(default)]
    price_impact_pct: Option<String>,
    #[serde(default)]
    route_plan: Vec<JupiterRoutePlan>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JupiterRoutePlan {
    #[serde(default)]
    swap_info: Option<JupiterSwapInfo>,
    #[serde(default)]
    percent: Option<f64>,
    #[serde(default)]
    bps: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JupiterSwapInfo {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    input_mint: Option<String>,
    #[serde(default)]
    output_mint: Option<String>,
    #[serde(default)]
    in_amount: Option<String>,
    #[serde(default)]
    out_amount: Option<String>,
}

#[async_trait]
impl ExchangeSource for JupiterQuotePoller {
    fn name(&self) -> &'static str {
        "jupiter"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                for amount in route_amounts(pair) {
                    match fetch_jupiter_quote(&self.client, &self.cfg.base_url, pair, amount).await
                    {
                        Ok(quote) => {
                            if amount == pair.amount {
                                if let Some(price) = quote_price(&quote, pair) {
                                    emit_defi_quote(
                                        &ctx,
                                        self.name(),
                                        &pair.symbol,
                                        price,
                                        pair.spread_bps,
                                    )
                                    .await?;
                                }
                            }
                            emit_jupiter_route_metrics(&ctx, pair, amount, &quote).await?;
                        }
                        Err(error) => {
                            warn!(symbol=%pair.symbol, amount, %error, "jupiter quote refresh failed");
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

fn route_amounts(pair: &SolanaQuotePair) -> Vec<u64> {
    let mut amounts = Vec::with_capacity(pair.route_amounts.len() + 1);
    amounts.push(pair.amount);
    for amount in &pair.route_amounts {
        if *amount > 0 && !amounts.contains(amount) {
            amounts.push(*amount);
        }
    }
    amounts
}

async fn fetch_jupiter_quote(
    client: &reqwest::Client,
    base_url: &str,
    pair: &SolanaQuotePair,
    amount: u64,
) -> Result<JupiterQuoteResponse> {
    let mut url = Url::parse(base_url)?.join("quote")?;
    url.query_pairs_mut()
        .append_pair("inputMint", &pair.input_mint)
        .append_pair("outputMint", &pair.output_mint)
        .append_pair("amount", &amount.to_string())
        .append_pair("swapMode", "ExactIn");

    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<JupiterQuoteResponse>()
        .await
        .context("failed to decode jupiter quote")
}

fn quote_price(quote: &JupiterQuoteResponse, pair: &SolanaQuotePair) -> Option<f64> {
    let in_amount = parse_f64_str(&quote.in_amount)?;
    let out_amount = parse_f64_str(&quote.out_amount)?;
    quote_to_price(
        in_amount,
        out_amount,
        pair.input_decimals,
        pair.output_decimals,
    )
}

async fn emit_jupiter_route_metrics(
    ctx: &SourceContext,
    pair: &SolanaQuotePair,
    amount: u64,
    quote: &JupiterQuoteResponse,
) -> Result<()> {
    let route_raw = serde_json::to_value(&quote.route_plan).ok();
    let input_metric = format!("route_input_amount_atomic_{amount}");
    let hops_metric = format!("route_hops_{amount}");
    let impact_metric = format!("route_price_impact_ratio_{amount}");
    let price_metric = format!("route_price_{amount}");
    let output_metric = format!("route_output_amount_atomic_{amount}");
    emit_defi_metric(
        ctx,
        "jupiter",
        &pair.symbol,
        &input_metric,
        Some(amount as f64),
        None,
    )
    .await?;
    emit_defi_metric(
        ctx,
        "jupiter",
        &pair.symbol,
        &hops_metric,
        Some(quote.route_plan.len() as f64),
        route_raw.clone(),
    )
    .await?;
    emit_defi_metric(
        ctx,
        "jupiter",
        &pair.symbol,
        &impact_metric,
        quote.price_impact_pct.as_deref().and_then(parse_f64_str),
        None,
    )
    .await?;
    emit_defi_metric(
        ctx,
        "jupiter",
        &pair.symbol,
        &price_metric,
        quote_price(quote, pair),
        route_raw,
    )
    .await?;
    emit_defi_metric(
        ctx,
        "jupiter",
        &pair.symbol,
        &output_metric,
        parse_f64_str(&quote.out_amount),
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jupiter_config_defaults_to_sol_usdc() {
        let cfg = JupiterConfig::default();
        assert_eq!(cfg.pairs[0].symbol, "SOLUSDC");
        assert_eq!(cfg.pairs[0].input_decimals, 9);
        assert_eq!(cfg.pairs[0].output_decimals, 6);
        assert!(cfg.pairs[0].route_amounts.is_empty());
    }

    #[test]
    fn route_amounts_keep_primary_and_deduplicate_invalid_sizes() {
        let mut pair = JupiterConfig::default().pairs.remove(0);
        pair.amount = 100;
        pair.route_amounts = vec![0, 200, 100, 200];
        assert_eq!(route_amounts(&pair), vec![100, 200]);
    }

    #[test]
    fn quote_price_uses_normalized_jupiter_amounts() {
        let pair = JupiterConfig::default().pairs[0].clone();
        let quote = JupiterQuoteResponse {
            in_amount: "1000000000".into(),
            out_amount: "250000000".into(),
            price_impact_pct: Some("0.001".into()),
            route_plan: Vec::new(),
        };
        assert_eq!(quote_price(&quote, &pair), Some(250.0));
    }
}
