use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use crate::config::{OrcaConfig, OrcaPair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_metric, emit_defi_quote};

pub struct OrcaWhirlpoolPoller {
    cfg: OrcaConfig,
    client: reqwest::Client,
}

impl OrcaWhirlpoolPoller {
    pub fn new(cfg: OrcaConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for OrcaWhirlpoolPoller {
    fn name(&self) -> &'static str {
        "orca"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            for pair in &self.cfg.pairs {
                match fetch_orca_pools(&self.client, &self.cfg, pair).await {
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
                        emit_orca_native_state(&ctx, pair, &page).await?;
                    }
                    Err(error) => {
                        warn!(symbol=%pair.symbol, %error, "orca whirlpool refresh failed")
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OrcaResponse {
    #[serde(default)]
    data: Vec<OrcaPool>,
    #[serde(default)]
    meta: Option<OrcaMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OrcaMeta {
    #[serde(default)]
    cursor: Option<OrcaCursor>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OrcaCursor {
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    previous: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrcaPool {
    #[serde(default)]
    address: String,
    #[serde(default)]
    pool_type: Option<String>,
    #[serde(default)]
    price: Option<Value>,
    #[serde(default)]
    tvl_usdc: Option<Value>,
    #[serde(default)]
    yield_over_tvl: Option<Value>,
    #[serde(default)]
    fee_rate: Option<Value>,
    #[serde(default)]
    has_warning: Option<bool>,
    #[serde(default)]
    adaptive_fee_enabled: Option<bool>,
    #[serde(default)]
    token_a: Option<OrcaToken>,
    #[serde(default)]
    token_b: Option<OrcaToken>,
    #[serde(default)]
    stats: HashMap<String, OrcaStats>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrcaToken {
    #[serde(default)]
    symbol: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrcaStats {
    #[serde(default)]
    volume: Option<Value>,
    #[serde(default)]
    fees: Option<Value>,
    #[serde(default)]
    yield_over_tvl: Option<Value>,
}

#[derive(Debug, Clone)]
struct OrcaPoolPage {
    pools: Vec<OrcaPool>,
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

fn pool_matches_query(pool: &OrcaPool, pair: &OrcaPair) -> bool {
    let tokens = query_tokens(&pair.query);
    if tokens.len() < 2 {
        return true;
    }
    let Some(token_a) = pool
        .token_a
        .as_ref()
        .and_then(|token| token.symbol.as_ref())
    else {
        return false;
    };
    let Some(token_b) = pool
        .token_b
        .as_ref()
        .and_then(|token| token.symbol.as_ref())
    else {
        return false;
    };
    (token_a.eq_ignore_ascii_case(&tokens[0]) && token_b.eq_ignore_ascii_case(&tokens[1]))
        || (token_a.eq_ignore_ascii_case(&tokens[1]) && token_b.eq_ignore_ascii_case(&tokens[0]))
}

async fn fetch_orca_pools(
    client: &reqwest::Client,
    cfg: &OrcaConfig,
    pair: &OrcaPair,
) -> Result<OrcaPoolPage> {
    let mut url = Url::parse(&cfg.native_base_url)?.join("pools/search")?;
    url.query_pairs_mut()
        .append_pair("q", &pair.query)
        .append_pair("size", &cfg.page_size.clamp(1, 100).to_string())
        .append_pair("stats", &cfg.stats)
        .append_pair("sortBy", "tvl")
        .append_pair("sortDirection", "desc");
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<OrcaResponse>()
        .await
        .context("failed to decode orca whirlpool response")?;
    let pools = response
        .data
        .into_iter()
        .filter(|pool| pool_matches_query(pool, pair))
        .collect::<Vec<_>>();
    if pools.is_empty() {
        bail!("no Orca Whirlpool matched query {}", pair.query);
    }
    Ok(OrcaPoolPage {
        pools,
        has_next_page: response
            .meta
            .and_then(|meta| meta.cursor)
            .map(|cursor| cursor.next.is_some()),
    })
}

fn select_top_pool(pools: &[OrcaPool]) -> Option<&OrcaPool> {
    pools.iter().max_by(|left, right| {
        value_number(left.tvl_usdc.as_ref())
            .zip(value_number(right.tvl_usdc.as_ref()))
            .map(|(left, right)| left.total_cmp(&right))
            .unwrap_or(Ordering::Equal)
    })
}

fn stats_24h(pool: &OrcaPool) -> Option<&OrcaStats> {
    pool.stats.get("24h")
}

fn metric_value(pool: &OrcaPool, metric: impl Fn(&OrcaPool) -> Option<&Value>) -> Option<f64> {
    value_number(metric(pool))
}

fn stat_value(pool: &OrcaPool, metric: impl Fn(&OrcaStats) -> Option<&Value>) -> Option<f64> {
    value_number(stats_24h(pool).and_then(metric))
}

fn sum_pool_metric<F>(pools: &[OrcaPool], metric: F) -> Option<f64>
where
    F: Fn(&OrcaPool) -> Option<f64>,
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

fn pair_price(pool: &OrcaPool, pair: &OrcaPair) -> Option<f64> {
    let price = metric_value(pool, |pool| pool.price.as_ref())?;
    if price <= 0.0 {
        return None;
    }
    let tokens = query_tokens(&pair.query);
    if tokens.len() < 2 {
        return Some(price);
    }
    let token_a = pool.token_a.as_ref()?.symbol.as_ref()?;
    let token_b = pool.token_b.as_ref()?.symbol.as_ref()?;
    if token_a.eq_ignore_ascii_case(&tokens[0]) && token_b.eq_ignore_ascii_case(&tokens[1]) {
        Some(price)
    } else if token_a.eq_ignore_ascii_case(&tokens[1]) && token_b.eq_ignore_ascii_case(&tokens[0]) {
        Some(1.0 / price)
    } else {
        None
    }
}

async fn emit_orca_native_state(
    ctx: &SourceContext,
    pair: &OrcaPair,
    page: &OrcaPoolPage,
) -> Result<()> {
    let top = select_top_pool(&page.pools);
    let total_tvl = sum_pool_metric(&page.pools, |pool| {
        metric_value(pool, |pool| pool.tvl_usdc.as_ref())
    });
    let total_volume = sum_pool_metric(&page.pools, |pool| {
        stat_value(pool, |stats| stats.volume.as_ref())
    });
    let total_fees = sum_pool_metric(&page.pools, |pool| {
        stat_value(pool, |stats| stats.fees.as_ref())
    });
    let top_tvl = top.and_then(|pool| metric_value(pool, |pool| pool.tvl_usdc.as_ref()));
    let top_volume = top.and_then(|pool| stat_value(pool, |stats| stats.volume.as_ref()));
    let top_turnover = top_volume
        .zip(top_tvl)
        .and_then(|(volume, tvl)| (tvl > 0.0).then_some(volume / tvl));
    let raw_top = top.and_then(|pool| serde_json::to_value(pool).ok());
    let metrics = [
        ("pool_count", Some(page.pools.len() as f64)),
        (
            "pool_search_has_next_page",
            page.has_next_page
                .map(|value| if value { 1.0 } else { 0.0 }),
        ),
        ("pool_tvl_usdc_total", total_tvl),
        ("pool_volume_24h_usdc_total", total_volume),
        ("pool_fees_24h_usdc_total", total_fees),
        ("pool_top_tvl_usdc", top_tvl),
        (
            "pool_top_tvl_share",
            top_tvl
                .zip(total_tvl)
                .and_then(|(top, total)| (total > 0.0).then_some(top / total)),
        ),
        ("pool_top_volume_24h_usdc", top_volume),
        (
            "pool_top_volume_share",
            top_volume
                .zip(total_volume)
                .and_then(|(top, total)| (total > 0.0).then_some(top / total)),
        ),
        (
            "pool_top_yield_over_tvl",
            top.and_then(|pool| metric_value(pool, |pool| pool.yield_over_tvl.as_ref())),
        ),
        (
            "pool_top_fee_rate_raw",
            top.and_then(|pool| metric_value(pool, |pool| pool.fee_rate.as_ref())),
        ),
        (
            "pool_top_price",
            top.and_then(|pool| pair_price(pool, pair)),
        ),
        ("pool_top_turnover_24h_ratio", top_turnover),
        (
            "pool_top_has_warning",
            top.and_then(|pool| pool.has_warning.map(|value| if value { 1.0 } else { 0.0 })),
        ),
        (
            "pool_top_adaptive_fee_enabled",
            top.and_then(|pool| {
                pool.adaptive_fee_enabled
                    .map(|value| if value { 1.0 } else { 0.0 })
            }),
        ),
    ];
    for (metric, value) in metrics {
        let raw = (metric == "pool_top_price")
            .then(|| raw_top.clone())
            .flatten();
        emit_defi_metric(ctx, "orca", &pair.symbol, metric, value, raw).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(token_a: &str, token_b: &str, price: &str, tvl: &str, volume: &str) -> OrcaPool {
        serde_json::from_value(serde_json::json!({
            "address": "pool-1",
            "poolType": "whirlpool",
            "price": price,
            "tvlUsdc": tvl,
            "yieldOverTvl": "0.001",
            "feeRate": 400,
            "hasWarning": false,
            "adaptiveFeeEnabled": true,
            "tokenA": {"symbol": token_a},
            "tokenB": {"symbol": token_b},
            "stats": {"24h": {"volume": volume, "fees": "12"}}
        }))
        .unwrap()
    }

    #[test]
    fn query_matches_token_pair_in_both_orientations() {
        let pair = OrcaPair {
            symbol: "SOLUSDC".to_string(),
            query: "SOL USDC".to_string(),
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
    fn top_pool_and_native_stats_are_numeric() {
        let pools = vec![
            pool("SOL", "USDC", "100", "1000", "2000"),
            pool("SOL", "USDC", "101", "500", "500"),
        ];
        let top = select_top_pool(&pools).unwrap();
        assert_eq!(
            metric_value(top, |pool| pool.tvl_usdc.as_ref()),
            Some(1000.0)
        );
        assert_eq!(stat_value(top, |stats| stats.volume.as_ref()), Some(2000.0));
        assert_eq!(
            sum_pool_metric(&pools, |pool| metric_value(pool, |pool| pool
                .tvl_usdc
                .as_ref())),
            Some(1500.0)
        );
        assert_eq!(
            sum_pool_metric(&pools, |pool| stat_value(pool, |stats| stats.fees.as_ref())),
            Some(24.0)
        );
    }

    #[test]
    fn decodes_orca_cursor_and_native_fields() {
        let payload = serde_json::json!({
            "data": [serde_json::json!({
                "address": "pool-1",
                "poolType": "whirlpool",
                "price": "100.5",
                "tvlUsdc": "1234.5",
                "tokenA": {"symbol": "SOL"},
                "tokenB": {"symbol": "USDC"},
                "stats": {"24h": {"volume": "4567.8", "fees": "3.2"}}
            })],
            "meta": {"cursor": {"next": "cursor-2", "previous": null}}
        });
        let response: OrcaResponse = serde_json::from_value(payload).unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(
            response.meta.unwrap().cursor.unwrap().next.as_deref(),
            Some("cursor-2")
        );
        assert_eq!(
            value_number(response.data[0].tvl_usdc.as_ref()),
            Some(1234.5)
        );
    }
}
