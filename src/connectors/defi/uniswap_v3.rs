use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::config::{UniswapV3Config, UniswapV3Pool};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_metric, emit_defi_quote, parse_f64_str};

pub struct UniswapV3PoolPoller {
    cfg: UniswapV3Config,
    client: reqwest::Client,
}

impl UniswapV3PoolPoller {
    pub fn new(cfg: UniswapV3Config) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GraphQlResponse {
    data: Option<GraphQlData>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphQlData {
    pool: Option<UniswapPoolData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UniswapPoolData {
    token0_price: String,
    token1_price: String,
    liquidity: Option<String>,
    total_value_locked_usd: Option<String>,
    volume_usd: Option<String>,
    tx_count: Option<String>,
}

#[async_trait]
impl ExchangeSource for UniswapV3PoolPoller {
    fn name(&self) -> &'static str {
        "uniswap_v3"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pool in &self.cfg.pools {
                match fetch_uniswap_pool_price(&self.client, &self.cfg.subgraph_url, pool).await {
                    Ok(data) => {
                        let Some(price) = parse_uniswap_price(&data, pool.invert) else {
                            continue;
                        };
                        emit_defi_quote(&ctx, self.name(), &pool.symbol, price, pool.spread_bps)
                            .await?;
                        emit_uniswap_pool_state(&ctx, self.name(), pool, &data).await?;
                    }
                    Err(error) => {
                        warn!(symbol=%pool.symbol, pool=%pool.pool_id, %error, "uniswap v3 pool refresh failed");
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_uniswap_pool_price(
    client: &reqwest::Client,
    subgraph_url: &str,
    pool: &UniswapV3Pool,
) -> Result<UniswapPoolData> {
    let body = json!({
        "query": "query Pool($id: ID!) { pool(id: $id) { token0Price token1Price liquidity totalValueLockedUSD volumeUSD txCount } }",
        "variables": { "id": pool.pool_id.to_ascii_lowercase() }
    });

    let response = client
        .post(subgraph_url)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<GraphQlResponse>()
        .await
        .context("failed to decode uniswap v3 subgraph response")?;

    if !response.errors.is_empty() {
        anyhow::bail!("uniswap v3 subgraph returned errors");
    }

    let data = response
        .data
        .and_then(|data| data.pool)
        .context("uniswap v3 pool not found")?;
    parse_uniswap_price(&data, pool.invert).context("invalid uniswap v3 pool price")?;
    Ok(data)
}

fn parse_uniswap_price(data: &UniswapPoolData, invert: bool) -> Option<f64> {
    if invert {
        parse_f64_str(&data.token0_price)
    } else {
        parse_f64_str(&data.token1_price)
    }
}

async fn emit_uniswap_pool_state(
    ctx: &SourceContext,
    source: &'static str,
    pool: &UniswapV3Pool,
    data: &UniswapPoolData,
) -> Result<()> {
    for (metric, value) in [
        (
            "pool_liquidity_raw",
            data.liquidity.as_deref().and_then(parse_f64_str),
        ),
        (
            "pool_tvl_usd",
            data.total_value_locked_usd
                .as_deref()
                .and_then(parse_f64_str),
        ),
        (
            "swap_volume_usd_lifetime",
            data.volume_usd.as_deref().and_then(parse_f64_str),
        ),
        (
            "swap_count_lifetime",
            data.tx_count.as_deref().and_then(parse_f64_str),
        ),
    ] {
        emit_defi_metric(
            ctx,
            source,
            &pool.symbol,
            metric,
            value,
            Some(json!({"pool_id": pool.pool_id})),
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_configured_pool_price_side() {
        let data = UniswapPoolData {
            token0_price: "0.0005".to_string(),
            token1_price: "2000".to_string(),
            liquidity: Some("123".to_string()),
            total_value_locked_usd: Some("1000000".to_string()),
            volume_usd: Some("25000000".to_string()),
            tx_count: Some("42".to_string()),
        };
        assert_eq!(parse_uniswap_price(&data, false), Some(2000.0));
        assert_eq!(parse_uniswap_price(&data, true), Some(0.0005));
        assert_eq!(
            data.total_value_locked_usd
                .as_deref()
                .and_then(parse_f64_str),
            Some(1_000_000.0)
        );
    }
}
