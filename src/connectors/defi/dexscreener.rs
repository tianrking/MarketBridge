use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::{DexScreenerConfig, DexScreenerPair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_metric, emit_defi_quote, parse_f64_str};

pub struct DexScreenerPoller {
    source_name: &'static str,
    cfg: DexScreenerConfig,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    #[serde(default)]
    pairs: Vec<SearchPair>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchPair {
    #[serde(default)]
    chain_id: String,
    #[serde(default)]
    dex_id: String,
    price_usd: Option<String>,
    price_native: Option<String>,
    pair_address: Option<String>,
    liquidity: Option<Liquidity>,
    volume: Option<WindowMetrics>,
    txns: Option<TxnWindows>,
}

#[derive(Debug, Deserialize, Clone)]
struct Liquidity {
    usd: Option<f64>,
    base: Option<f64>,
    quote: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
struct WindowMetrics {
    h24: Option<f64>,
    h6: Option<f64>,
    h1: Option<f64>,
    m5: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
struct TxnWindows {
    h24: Option<TxnCounts>,
    h6: Option<TxnCounts>,
    h1: Option<TxnCounts>,
    m5: Option<TxnCounts>,
}

#[derive(Debug, Deserialize, Clone)]
struct TxnCounts {
    buys: Option<f64>,
    sells: Option<f64>,
}

impl DexScreenerPoller {
    pub fn new(source_name: &'static str, cfg: DexScreenerConfig) -> Self {
        Self {
            source_name,
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for DexScreenerPoller {
    fn name(&self) -> &'static str {
        self.source_name
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                match fetch_dexscreener_pair(&self.client, &self.cfg.base_url, pair).await {
                    Ok(row) => {
                        let Some(price) = pair_price(&row) else {
                            continue;
                        };
                        emit_defi_quote(&ctx, self.name(), &pair.symbol, price, pair.spread_bps)
                            .await?;
                        emit_dexscreener_native_state(&ctx, self.name(), pair, &row).await?;
                    }
                    Err(error) => warn!(
                        source = self.source_name,
                        symbol=%pair.symbol,
                        %error,
                        "dexscreener refresh failed"
                    ),
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_dexscreener_pair(
    client: &reqwest::Client,
    base_url: &str,
    pair: &DexScreenerPair,
) -> Result<SearchPair> {
    let mut url = Url::parse(base_url)?.join("latest/dex/search")?;
    url.query_pairs_mut().append_pair("q", &pair.query);
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<SearchResponse>()
        .await
        .context("failed to decode dexscreener search response")?;
    select_pair(&response, pair)
        .cloned()
        .context("matching dexscreener pair not found")
}

fn select_pair<'a>(response: &'a SearchResponse, pair: &DexScreenerPair) -> Option<&'a SearchPair> {
    response.pairs.iter().find(|candidate| {
        candidate.chain_id.eq_ignore_ascii_case(&pair.chain_id)
            && candidate.dex_id.eq_ignore_ascii_case(&pair.dex_id)
    })
}

fn pair_price(candidate: &SearchPair) -> Option<f64> {
    candidate
        .price_usd
        .as_deref()
        .or(candidate.price_native.as_deref())
        .and_then(parse_f64_str)
}

async fn emit_dexscreener_native_state(
    ctx: &SourceContext,
    source: &'static str,
    pair: &DexScreenerPair,
    row: &SearchPair,
) -> Result<()> {
    if let Some(liquidity) = &row.liquidity {
        emit_defi_metric(
            ctx,
            source,
            &pair.symbol,
            "pool_liquidity_usd",
            liquidity.usd,
            row.pair_address
                .as_ref()
                .map(|address| serde_json::json!({"pair_address": address})),
        )
        .await?;
        emit_defi_metric(
            ctx,
            source,
            &pair.symbol,
            "pool_liquidity_base",
            liquidity.base,
            None,
        )
        .await?;
        emit_defi_metric(
            ctx,
            source,
            &pair.symbol,
            "pool_liquidity_quote",
            liquidity.quote,
            None,
        )
        .await?;
    }
    if let Some(volume) = &row.volume {
        emit_window_metrics(ctx, source, &pair.symbol, "swap_volume", volume).await?;
    }
    if let Some(txns) = &row.txns {
        emit_txn_metrics(ctx, source, &pair.symbol, txns).await?;
    }
    Ok(())
}

async fn emit_window_metrics(
    ctx: &SourceContext,
    source: &'static str,
    symbol: &str,
    prefix: &str,
    metrics: &WindowMetrics,
) -> Result<()> {
    for (window, value) in [
        ("m5", metrics.m5),
        ("h1", metrics.h1),
        ("h6", metrics.h6),
        ("h24", metrics.h24),
    ] {
        emit_defi_metric(
            ctx,
            source,
            symbol,
            &format!("{prefix}_{window}"),
            value,
            None,
        )
        .await?;
    }
    Ok(())
}

async fn emit_txn_metrics(
    ctx: &SourceContext,
    source: &'static str,
    symbol: &str,
    txns: &TxnWindows,
) -> Result<()> {
    for (window, counts) in [
        ("m5", txns.m5.as_ref()),
        ("h1", txns.h1.as_ref()),
        ("h6", txns.h6.as_ref()),
        ("h24", txns.h24.as_ref()),
    ] {
        if let Some(counts) = counts {
            emit_defi_metric(
                ctx,
                source,
                symbol,
                &format!("swap_buys_{window}"),
                counts.buys,
                None,
            )
            .await?;
            emit_defi_metric(
                ctx,
                source,
                symbol,
                &format!("swap_sells_{window}"),
                counts.sells,
                None,
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dexscreener_selects_matching_dex_pair_price() {
        let response = SearchResponse {
            pairs: vec![
                SearchPair {
                    chain_id: "solana".to_string(),
                    dex_id: "raydium".to_string(),
                    price_usd: Some("199".to_string()),
                    price_native: None,
                    pair_address: None,
                    liquidity: None,
                    volume: None,
                    txns: None,
                },
                SearchPair {
                    chain_id: "solana".to_string(),
                    dex_id: "meteora".to_string(),
                    price_usd: Some("200".to_string()),
                    price_native: None,
                    pair_address: Some("abc".to_string()),
                    liquidity: Some(Liquidity {
                        usd: Some(1_000_000.0),
                        base: Some(100.0),
                        quote: Some(200_000.0),
                    }),
                    volume: Some(WindowMetrics {
                        h24: Some(50_000.0),
                        h6: None,
                        h1: None,
                        m5: None,
                    }),
                    txns: Some(TxnWindows {
                        h24: Some(TxnCounts {
                            buys: Some(20.0),
                            sells: Some(15.0),
                        }),
                        h6: None,
                        h1: None,
                        m5: None,
                    }),
                },
            ],
        };
        let pair = DexScreenerPair {
            symbol: "SOLUSDC".to_string(),
            chain_id: "solana".to_string(),
            dex_id: "meteora".to_string(),
            query: "SOL USDC".to_string(),
            spread_bps: 5.0,
        };
        let selected = select_pair(&response, &pair).expect("selected");
        assert_eq!(pair_price(selected), Some(200.0));
        assert_eq!(
            selected.liquidity.as_ref().and_then(|x| x.usd),
            Some(1_000_000.0)
        );
        assert_eq!(
            selected
                .txns
                .as_ref()
                .and_then(|x| x.h24.as_ref())
                .and_then(|x| x.buys),
            Some(20.0)
        );
    }
}
