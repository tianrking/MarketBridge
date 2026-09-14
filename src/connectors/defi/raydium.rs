use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::{RaydiumConfig, RaydiumPair};
use crate::source::{ExchangeSource, SourceContext};

use super::common::{emit_defi_metric, emit_defi_quote};

pub struct RaydiumPricePoller {
    cfg: RaydiumConfig,
    client: reqwest::Client,
}

impl RaydiumPricePoller {
    pub fn new(cfg: RaydiumConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for RaydiumPricePoller {
    fn name(&self) -> &'static str {
        "raydium"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_raydium_prices(&self.client, &self.cfg.price_url).await {
                Ok(prices) => {
                    for pair in &self.cfg.pairs {
                        match pair_price(&prices, pair) {
                            Some(price) => {
                                emit_defi_quote(
                                    &ctx,
                                    self.name(),
                                    &pair.symbol,
                                    price,
                                    pair.spread_bps,
                                )
                                .await?;
                            }
                            None => warn!(
                                symbol=%pair.symbol,
                                "raydium price pair missing base or quote mint"
                            ),
                        }
                    }
                }
                Err(error) => {
                    warn!(%error, "raydium price refresh failed");
                }
            }
            for pair in &self.cfg.pairs {
                match fetch_raydium_pool_page(&self.client, &self.cfg, pair).await {
                    Ok(page) => emit_raydium_pool_metrics(&ctx, pair, &page).await?,
                    Err(error) => {
                        warn!(symbol=%pair.symbol, %error, "raydium pool state refresh failed")
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RaydiumPoolResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<RaydiumPoolPage>,
    #[serde(default)]
    msg: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RaydiumPoolPage {
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    data: Vec<RaydiumPool>,
    #[serde(rename = "hasNextPage", default)]
    has_next_page: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RaydiumPool {
    #[serde(default)]
    id: String,
    #[serde(rename = "type", default)]
    pool_type: Option<String>,
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    tvl: Option<f64>,
    #[serde(rename = "feeRate", default)]
    fee_rate: Option<f64>,
    #[serde(default)]
    day: Option<RaydiumDayStats>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RaydiumDayStats {
    #[serde(default)]
    volume: Option<f64>,
    #[serde(rename = "volumeFee", default)]
    volume_fee: Option<f64>,
}

#[derive(Debug, Clone)]
struct RaydiumPoolSummary {
    page_count: usize,
    total_count: Option<usize>,
    has_next_page: Option<bool>,
    page_tvl_total: Option<f64>,
    page_volume_24h_total: Option<f64>,
    page_fee_24h_total: Option<f64>,
    top_tvl: Option<f64>,
    top_tvl_share: Option<f64>,
    top_volume_24h: Option<f64>,
    top_volume_share: Option<f64>,
    top_fee_rate: Option<f64>,
    top_price: Option<f64>,
    top_turnover_24h: Option<f64>,
    top_pool_id: Option<String>,
    top_pool_type: Option<String>,
}

async fn fetch_raydium_prices(
    client: &reqwest::Client,
    price_url: &str,
) -> Result<HashMap<String, f64>> {
    client
        .get(price_url)
        .send()
        .await?
        .error_for_status()?
        .json::<HashMap<String, f64>>()
        .await
        .context("failed to decode raydium price map")
}

async fn fetch_raydium_pool_page(
    client: &reqwest::Client,
    cfg: &RaydiumConfig,
    pair: &RaydiumPair,
) -> Result<RaydiumPoolPage> {
    let mut url = Url::parse(&cfg.pool_info_url)?;
    url.query_pairs_mut()
        .append_pair("mint1", &pair.base_mint)
        .append_pair("mint2", &pair.quote_mint)
        .append_pair("poolType", &cfg.pool_type)
        .append_pair("poolSortField", &cfg.pool_sort_field)
        .append_pair("sortType", &cfg.sort_type)
        .append_pair("pageSize", &cfg.pool_page_size.clamp(1, 1000).to_string())
        .append_pair("page", "1");
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<RaydiumPoolResponse>()
        .await
        .context("failed to decode raydium pool info")?;
    if !response.success {
        bail!(
            "raydium pool info error: {}",
            response.msg.as_deref().unwrap_or("unknown provider error")
        );
    }
    response
        .data
        .context("raydium pool info response omitted data")
}

fn valid_nonnegative(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn sum_pool_metric<F>(pools: &[RaydiumPool], metric: F) -> Option<f64>
where
    F: Fn(&RaydiumPool) -> Option<f64>,
{
    let mut total = 0.0;
    let mut seen = 0;
    for pool in pools {
        if let Some(value) = metric(pool).and_then(|value| valid_nonnegative(Some(value))) {
            total += value;
            seen += 1;
        }
    }
    (seen > 0).then_some(total)
}

fn summarize_pool_page(page: &RaydiumPoolPage) -> RaydiumPoolSummary {
    let mut ranked: Vec<&RaydiumPool> = page
        .data
        .iter()
        .filter(|pool| valid_nonnegative(pool.tvl).is_some())
        .collect();
    ranked.sort_by(|left, right| {
        valid_nonnegative(right.tvl)
            .and_then(|right| valid_nonnegative(left.tvl).map(|left| right.total_cmp(&left)))
            .unwrap_or(Ordering::Equal)
    });
    let top = ranked.first().copied();
    let page_tvl_total = sum_pool_metric(&page.data, |pool| pool.tvl);
    let page_volume_24h_total = sum_pool_metric(&page.data, |pool| {
        pool.day.as_ref().and_then(|day| day.volume)
    });
    let page_fee_24h_total = sum_pool_metric(&page.data, |pool| {
        pool.day.as_ref().and_then(|day| day.volume_fee)
    });
    let top_tvl = top.and_then(|pool| valid_nonnegative(pool.tvl));
    let top_volume_24h = top.and_then(|pool| {
        pool.day
            .as_ref()
            .and_then(|day| valid_nonnegative(day.volume))
    });
    let top_turnover_24h = top_volume_24h
        .zip(top_tvl)
        .and_then(|(volume, tvl)| (tvl > 0.0).then_some(volume / tvl));
    RaydiumPoolSummary {
        page_count: page.data.len(),
        total_count: page.count,
        has_next_page: page.has_next_page,
        page_tvl_total,
        page_volume_24h_total,
        page_fee_24h_total,
        top_tvl,
        top_tvl_share: top_tvl
            .zip(page_tvl_total)
            .and_then(|(top, total)| (total > 0.0).then_some(top / total)),
        top_volume_24h,
        top_volume_share: top_volume_24h
            .zip(page_volume_24h_total)
            .and_then(|(top, total)| (total > 0.0).then_some(top / total)),
        top_fee_rate: top.and_then(|pool| valid_nonnegative(pool.fee_rate)),
        top_price: top
            .and_then(|pool| pool.price.filter(|price| price.is_finite() && *price > 0.0)),
        top_turnover_24h,
        top_pool_id: top.map(|pool| pool.id.clone()),
        top_pool_type: top.and_then(|pool| pool.pool_type.clone()),
    }
}

async fn emit_raydium_pool_metrics(
    ctx: &SourceContext,
    pair: &RaydiumPair,
    page: &RaydiumPoolPage,
) -> Result<()> {
    let summary = summarize_pool_page(page);
    let top_raw = match (&summary.top_pool_id, &summary.top_pool_type) {
        (Some(id), pool_type) => Some(serde_json::json!({
            "id": id,
            "type": pool_type,
        })),
        _ => None,
    };
    let metrics = [
        ("pool_page_count", Some(summary.page_count as f64)),
        (
            "pool_catalog_count",
            summary.total_count.map(|value| value as f64),
        ),
        (
            "pool_has_next_page",
            summary
                .has_next_page
                .map(|value| if value { 1.0 } else { 0.0 }),
        ),
        (
            "pool_page_coverage_ratio",
            summary
                .total_count
                .and_then(|total| (total > 0).then_some(summary.page_count as f64 / total as f64)),
        ),
        ("pool_tvl_usd_page_total", summary.page_tvl_total),
        (
            "pool_volume_24h_usd_page_total",
            summary.page_volume_24h_total,
        ),
        ("pool_fee_24h_usd_page_total", summary.page_fee_24h_total),
        ("pool_top_tvl_usd", summary.top_tvl),
        ("pool_top_tvl_share_page", summary.top_tvl_share),
        ("pool_top_volume_24h_usd", summary.top_volume_24h),
        ("pool_top_volume_share_page", summary.top_volume_share),
        ("pool_top_fee_rate", summary.top_fee_rate),
        ("pool_top_price", summary.top_price),
        (
            "pool_turnover_24h_page_ratio",
            summary
                .page_volume_24h_total
                .zip(summary.page_tvl_total)
                .and_then(|(volume, tvl)| (tvl > 0.0).then_some(volume / tvl)),
        ),
        ("pool_top_turnover_24h_ratio", summary.top_turnover_24h),
    ];
    for (metric, value) in metrics {
        let raw = (metric == "pool_top_price")
            .then(|| top_raw.clone())
            .flatten();
        emit_defi_metric(ctx, "raydium", &pair.symbol, metric, value, raw).await?;
    }
    Ok(())
}

fn pair_price(prices: &HashMap<String, f64>, pair: &RaydiumPair) -> Option<f64> {
    let base = prices.get(&pair.base_mint)?;
    let quote = prices.get(&pair.quote_mint)?;
    if *base <= 0.0 || *quote <= 0.0 {
        return None;
    }
    Some(base / quote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_pair_price_from_usd_prices() {
        let pair = RaydiumPair {
            symbol: "SOLUSDC".to_string(),
            base_mint: "SOL".to_string(),
            quote_mint: "USDC".to_string(),
            spread_bps: 5.0,
        };
        let prices = HashMap::from([("SOL".to_string(), 150.0), ("USDC".to_string(), 1.0)]);
        assert_eq!(pair_price(&prices, &pair), Some(150.0));
    }

    #[test]
    fn raydium_defaults_to_native_pool_info_endpoint() {
        let cfg = RaydiumConfig::default();
        assert!(cfg.pool_info_url.ends_with("/pools/info/mint"));
        assert_eq!(cfg.pool_type, "all");
        assert_eq!(cfg.pool_page_size, 1000);
    }

    fn sample_pool(id: &str, tvl: f64, volume: f64) -> RaydiumPool {
        RaydiumPool {
            id: id.to_string(),
            pool_type: Some("Standard".to_string()),
            price: Some(100.0),
            tvl: Some(tvl),
            fee_rate: Some(0.0025),
            day: Some(RaydiumDayStats {
                volume: Some(volume),
                volume_fee: Some(volume * 0.0025),
            }),
        }
    }

    #[test]
    fn pool_summary_reports_page_totals_and_top_concentration() {
        let page = RaydiumPoolPage {
            count: Some(2),
            data: vec![
                sample_pool("top", 80.0, 160.0),
                sample_pool("small", 20.0, 40.0),
            ],
            has_next_page: Some(false),
        };
        let summary = summarize_pool_page(&page);
        assert_eq!(summary.page_count, 2);
        assert_eq!(summary.page_tvl_total, Some(100.0));
        assert_eq!(summary.page_volume_24h_total, Some(200.0));
        assert_eq!(summary.top_tvl_share, Some(0.8));
        assert_eq!(summary.top_volume_share, Some(0.8));
        assert_eq!(summary.top_turnover_24h, Some(2.0));
    }

    #[test]
    fn pool_summary_keeps_missing_catalog_count_explicit() {
        let page = RaydiumPoolPage {
            count: None,
            data: vec![sample_pool("only", 10.0, 1.0)],
            has_next_page: None,
        };
        let summary = summarize_pool_page(&page);
        assert_eq!(summary.total_count, None);
        assert_eq!(summary.page_tvl_total, Some(10.0));
        assert_eq!(summary.top_tvl_share, Some(1.0));
    }

    #[test]
    fn decodes_api_v3_pool_page_shape() {
        let payload = serde_json::json!({
            "success": true,
            "data": {
                "count": 1,
                "hasNextPage": false,
                "data": [{
                    "id": "pool-1",
                    "type": "Concentrated",
                    "price": 100.0,
                    "tvl": 12345.0,
                    "feeRate": 0.0004,
                    "day": {"volume": 67890.0, "volumeFee": 27.156}
                }]
            }
        });
        let response: RaydiumPoolResponse = serde_json::from_value(payload).unwrap();
        let page = response.data.unwrap();
        assert_eq!(page.count, Some(1));
        assert_eq!(page.has_next_page, Some(false));
        assert_eq!(page.data[0].pool_type.as_deref(), Some("Concentrated"));
        assert_eq!(page.data[0].day.as_ref().unwrap().volume_fee, Some(27.156));
    }
}
