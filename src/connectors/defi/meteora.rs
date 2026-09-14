use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use crate::config::{MeteoraConfig, MeteoraPair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_metric, emit_defi_quote};

pub struct MeteoraDlmmPoller {
    cfg: MeteoraConfig,
    client: reqwest::Client,
}

impl MeteoraDlmmPoller {
    pub fn new(cfg: MeteoraConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for MeteoraDlmmPoller {
    fn name(&self) -> &'static str {
        "meteora"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                match fetch_meteora_pools(&self.client, &self.cfg, pair).await {
                    Ok(page) => {
                        if let Some(pool) = select_top_pool(&page.pools)
                            && let Some(price) = pair_price(pool, pair)
                        {
                            emit_defi_quote(
                                &ctx,
                                self.name(),
                                &pair.symbol,
                                price,
                                pair.spread_bps,
                            )
                            .await?;
                        }
                        emit_meteora_native_state(&ctx, pair, &page).await?;
                    }
                    Err(error) => {
                        warn!(symbol=%pair.symbol, %error, "meteora DLMM refresh failed")
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MeteoraResponse {
    #[serde(default)]
    current_page: Option<usize>,
    #[serde(default)]
    data: Vec<MeteoraPool>,
    #[serde(default)]
    page_size: Option<usize>,
    #[serde(default)]
    pages: Option<usize>,
    #[serde(default)]
    total: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MeteoraPool {
    #[serde(default)]
    address: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    current_price: Option<Value>,
    #[serde(default)]
    dynamic_fee_pct: Option<Value>,
    #[serde(default)]
    tvl: Option<Value>,
    #[serde(default)]
    bin_step: Option<Value>,
    #[serde(default)]
    is_blacklisted: Option<bool>,
    #[serde(default)]
    fee_tvl_ratio: HashMap<String, Value>,
    #[serde(default)]
    fees: HashMap<String, Value>,
    #[serde(default)]
    volume: HashMap<String, Value>,
    #[serde(default)]
    pool_config: Option<MeteoraPoolConfig>,
    #[serde(default)]
    token_x: Option<MeteoraToken>,
    #[serde(default)]
    token_y: Option<MeteoraToken>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MeteoraPoolConfig {
    #[serde(default)]
    base_fee_pct: Option<Value>,
    #[serde(default)]
    max_fee_pct: Option<Value>,
    #[serde(default)]
    bin_step: Option<Value>,
    #[serde(default)]
    protocol_fee_pct: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MeteoraToken {
    #[serde(default)]
    symbol: Option<String>,
}

#[derive(Debug, Clone)]
struct MeteoraPoolPage {
    pools: Vec<MeteoraPool>,
    current_page: Option<usize>,
    page_size: Option<usize>,
    pages: Option<usize>,
    total: Option<usize>,
    has_next_page: Option<bool>,
}

fn value_number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
        .filter(|number| number.is_finite())
}

fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_uppercase())
        .collect()
}

fn pool_matches_query(pool: &MeteoraPool, pair: &MeteoraPair) -> bool {
    let tokens = query_tokens(&pair.query);
    if tokens.len() < 2 {
        return true;
    }
    let Some(token_x) = pool
        .token_x
        .as_ref()
        .and_then(|token| token.symbol.as_ref())
    else {
        return false;
    };
    let Some(token_y) = pool
        .token_y
        .as_ref()
        .and_then(|token| token.symbol.as_ref())
    else {
        return false;
    };
    (token_x.eq_ignore_ascii_case(&tokens[0]) && token_y.eq_ignore_ascii_case(&tokens[1]))
        || (token_x.eq_ignore_ascii_case(&tokens[1]) && token_y.eq_ignore_ascii_case(&tokens[0]))
}

async fn fetch_meteora_pools(
    client: &reqwest::Client,
    cfg: &MeteoraConfig,
    pair: &MeteoraPair,
) -> Result<MeteoraPoolPage> {
    let mut url = Url::parse(&cfg.native_base_url)?.join("pools")?;
    url.query_pairs_mut()
        .append_pair("query", &pair.query)
        .append_pair("page_size", &cfg.page_size.clamp(1, 1000).to_string())
        .append_pair("page", "1")
        .append_pair("sort_by", "tvl:desc");
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<MeteoraResponse>()
        .await
        .context("failed to decode Meteora DLMM response")?;
    let pools = response
        .data
        .into_iter()
        .filter(|pool| pool_matches_query(pool, pair))
        .collect::<Vec<_>>();
    if pools.is_empty() {
        bail!("no Meteora DLMM pool matched query {}", pair.query);
    }
    let has_next_page = response
        .current_page
        .zip(response.pages)
        .map(|(current, pages)| current < pages);
    Ok(MeteoraPoolPage {
        pools,
        current_page: response.current_page,
        page_size: response.page_size,
        pages: response.pages,
        total: response.total,
        has_next_page,
    })
}

fn select_top_pool(pools: &[MeteoraPool]) -> Option<&MeteoraPool> {
    pools.iter().max_by(|left, right| {
        value_number(left.tvl.as_ref())
            .zip(value_number(right.tvl.as_ref()))
            .map(|(left, right)| left.total_cmp(&right))
            .unwrap_or(Ordering::Equal)
    })
}

fn window_value(values: &HashMap<String, Value>, window: &str) -> Option<f64> {
    value_number(values.get(window))
}

fn pair_price(pool: &MeteoraPool, pair: &MeteoraPair) -> Option<f64> {
    let price = value_number(pool.current_price.as_ref())?;
    if price <= 0.0 {
        return None;
    }
    let tokens = query_tokens(&pair.query);
    if tokens.len() < 2 {
        return Some(price);
    }
    let token_x = pool.token_x.as_ref()?.symbol.as_ref()?;
    let token_y = pool.token_y.as_ref()?.symbol.as_ref()?;
    if token_x.eq_ignore_ascii_case(&tokens[0]) && token_y.eq_ignore_ascii_case(&tokens[1]) {
        Some(price)
    } else if token_x.eq_ignore_ascii_case(&tokens[1]) && token_y.eq_ignore_ascii_case(&tokens[0]) {
        Some(1.0 / price)
    } else {
        None
    }
}

fn sum_pool_metric<F>(pools: &[MeteoraPool], metric: F) -> Option<f64>
where
    F: Fn(&MeteoraPool) -> Option<f64>,
{
    let mut total = 0.0;
    let mut count = 0;
    for pool in pools {
        if let Some(value) = metric(pool).filter(|value| *value >= 0.0) {
            total += value;
            count += 1;
        }
    }
    (count > 0).then_some(total)
}

async fn emit_meteora_native_state(
    ctx: &SourceContext,
    pair: &MeteoraPair,
    page: &MeteoraPoolPage,
) -> Result<()> {
    let top = select_top_pool(&page.pools);
    let total_tvl = sum_pool_metric(&page.pools, |pool| value_number(pool.tvl.as_ref()));
    let total_volume = sum_pool_metric(&page.pools, |pool| window_value(&pool.volume, "24h"));
    let total_fees = sum_pool_metric(&page.pools, |pool| window_value(&pool.fees, "24h"));
    let top_tvl = top.and_then(|pool| value_number(pool.tvl.as_ref()));
    let top_volume = top.and_then(|pool| window_value(&pool.volume, "24h"));
    let top_fees = top.and_then(|pool| window_value(&pool.fees, "24h"));
    let raw_top = top.and_then(|pool| serde_json::to_value(pool).ok());
    let metrics = [
        ("pool_count", Some(page.pools.len() as f64)),
        (
            "pool_current_page",
            page.current_page.map(|value| value as f64),
        ),
        ("pool_page_size", page.page_size.map(|value| value as f64)),
        ("pool_pages", page.pages.map(|value| value as f64)),
        ("pool_total_count", page.total.map(|value| value as f64)),
        (
            "pool_has_next_page",
            page.has_next_page
                .map(|value| if value { 1.0 } else { 0.0 }),
        ),
        ("pool_tvl_total", total_tvl),
        ("pool_volume_24h_total", total_volume),
        ("pool_fees_24h_total", total_fees),
        ("pool_top_tvl", top_tvl),
        (
            "pool_top_tvl_share",
            top_tvl
                .zip(total_tvl)
                .and_then(|(top, total)| (total > 0.0).then_some(top / total)),
        ),
        ("pool_top_volume_24h", top_volume),
        ("pool_top_fees_24h", top_fees),
        (
            "pool_top_fee_tvl_ratio_24h",
            top.and_then(|pool| window_value(&pool.fee_tvl_ratio, "24h")),
        ),
        (
            "pool_top_turnover_24h_ratio",
            top_volume
                .zip(top_tvl)
                .and_then(|(volume, tvl)| (tvl > 0.0).then_some(volume / tvl)),
        ),
        (
            "pool_top_current_price",
            top.and_then(|pool| pair_price(pool, pair)),
        ),
        (
            "pool_top_dynamic_fee_pct",
            top.and_then(|pool| value_number(pool.dynamic_fee_pct.as_ref())),
        ),
        (
            "pool_top_bin_step",
            top.and_then(|pool| {
                value_number(pool.bin_step.as_ref()).or_else(|| {
                    pool.pool_config
                        .as_ref()
                        .and_then(|config| value_number(config.bin_step.as_ref()))
                })
            }),
        ),
        (
            "pool_top_is_blacklisted",
            top.and_then(|pool| {
                pool.is_blacklisted
                    .map(|value| if value { 1.0 } else { 0.0 })
            }),
        ),
    ];
    for (metric, value) in metrics {
        let raw = (metric == "pool_top_current_price")
            .then(|| raw_top.clone())
            .flatten();
        emit_defi_metric(ctx, "meteora", &pair.symbol, metric, value, raw).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(token_x: &str, token_y: &str, price: &str, tvl: &str, volume: &str) -> MeteoraPool {
        serde_json::from_value(serde_json::json!({
            "address": "pool-1",
            "name": "SOL-USDC",
            "current_price": price,
            "dynamic_fee_pct": "0.12",
            "tvl": tvl,
            "bin_step": 10,
            "is_blacklisted": false,
            "fee_tvl_ratio": {"24h": "0.08"},
            "fees": {"24h": "12"},
            "volume": {"24h": volume},
            "pool_config": {"bin_step": 10},
            "token_x": {"symbol": token_x},
            "token_y": {"symbol": token_y}
        }))
        .unwrap()
    }

    #[test]
    fn query_matches_pair_in_both_orientations() {
        let pair = MeteoraPair {
            symbol: "SOLUSDC".into(),
            query: "SOL USDC".into(),
            spread_bps: 5.0,
        };
        let forward = pool("SOL", "USDC", "100", "1000", "2000");
        let reverse = pool("USDC", "SOL", "0.01", "1000", "2000");
        assert!(pool_matches_query(&forward, &pair));
        assert!(pool_matches_query(&reverse, &pair));
        assert_eq!(pair_price(&forward, &pair), Some(100.0));
        assert_eq!(pair_price(&reverse, &pair), Some(100.0));
    }

    #[test]
    fn top_pool_aggregates_24h_metrics() {
        let pools = vec![
            pool("SOL", "USDC", "100", "1000", "2000"),
            pool("SOL", "USDC", "101", "500", "500"),
        ];
        let top = select_top_pool(&pools).unwrap();
        assert_eq!(value_number(top.tvl.as_ref()), Some(1000.0));
        assert_eq!(window_value(&top.volume, "24h"), Some(2000.0));
        assert_eq!(
            sum_pool_metric(&pools, |pool| value_number(pool.tvl.as_ref())),
            Some(1500.0)
        );
        assert_eq!(
            sum_pool_metric(&pools, |pool| window_value(&pool.fees, "24h")),
            Some(24.0)
        );
    }

    #[test]
    fn decodes_official_pagination_shape() {
        let payload = serde_json::json!({
            "current_page": 1,
            "page_size": 50,
            "pages": 3,
            "total": 101,
            "data": [serde_json::json!({
                "address": "pool-1",
                "current_price": "100.5",
                "tvl": "1234.5",
                "fee_tvl_ratio": {"24h": "0.03"},
                "token_x": {"symbol": "SOL"},
                "token_y": {"symbol": "USDC"}
            })]
        });
        let response: MeteoraResponse = serde_json::from_value(payload).unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.current_page, Some(1));
        assert_eq!(response.pages, Some(3));
        assert_eq!(value_number(response.data[0].tvl.as_ref()), Some(1234.5));
    }
}
