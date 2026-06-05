mod common;

use std::collections::HashMap;

use anyhow::Result;
use common::{
    Args, get_json, iteration_done, marketbridge_symbol_to_aggregate_symbol, matches_exchange,
    number, oi_change_pct, print_score, rows, signed_liquidation_notional, sleep_duration, text,
    ts_ms,
};
use reqwest::Client;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::new();
    let mut previous_oi = HashMap::new();
    let aggregate_symbol = marketbridge_symbol_to_aggregate_symbol(&args.symbol);

    println!(
        "short squeeze monitor | symbol={} aggregate_symbol={} exchange={}",
        args.symbol,
        aggregate_symbol,
        args.exchange.as_deref().unwrap_or("all")
    );

    for iteration in 0usize.. {
        let snapshot = Snapshot::load(&client, &args, &aggregate_symbol, &mut previous_oi).await?;
        snapshot.report();

        if iteration_done(iteration, &args) {
            break;
        }
        tokio::time::sleep(sleep_duration(&args)).await;
    }

    Ok(())
}

#[derive(Debug, Default)]
struct Snapshot {
    min_funding_rate: Option<f64>,
    max_oi_change_pct: Option<f64>,
    spot_cvd_delta: Option<f64>,
    perp_cvd_delta: Option<f64>,
    upper_liquidation_notional: Option<f64>,
    coinglass_liquidation_value: Option<f64>,
    latest_price: Option<f64>,
    reasons: Vec<String>,
}

impl Snapshot {
    async fn load(
        client: &Client,
        args: &Args,
        aggregate_symbol: &str,
        previous_oi: &mut HashMap<String, f64>,
    ) -> Result<Self> {
        let funding = get_json(
            client,
            &args.base_url,
            &format!("/v1/market/funding?symbols={}", args.symbol),
        )
        .await?;
        let open_interest = get_json(
            client,
            &args.base_url,
            &format!("/v1/market/open-interest?symbols={}", args.symbol),
        )
        .await?;
        let spot_flow = get_json(
            client,
            &args.base_url,
            &format!(
                "/v1/market/order-flow?market=spot&symbol={}&window_ms=60000&limit=50",
                args.symbol
            ),
        )
        .await?;
        let perp_flow = get_json(
            client,
            &args.base_url,
            &format!(
                "/v1/market/order-flow?market=perp&symbol={}&window_ms=60000&limit=50",
                args.symbol
            ),
        )
        .await?;
        let liquidations = get_json(
            client,
            &args.base_url,
            &format!("/v1/market/liquidations?symbols={}", args.symbol),
        )
        .await?;
        let quotes = get_json(
            client,
            &args.base_url,
            &format!("/v1/market/quotes?symbols={}", args.symbol),
        )
        .await?;
        let external = get_json(
            client,
            &args.base_url,
            &format!(
                "/v1/external/signals?sources=coinglass&symbols={aggregate_symbol}&metrics=liquidation"
            ),
        )
        .await
        .ok();

        let min_funding_rate = rows(&funding, "funding")
            .into_iter()
            .filter(|row| matches_exchange(row, &args.exchange))
            .filter_map(|row| number(row, "funding_rate"))
            .min_by(f64::total_cmp);

        let max_oi_change_pct = rows(&open_interest, "open_interest")
            .into_iter()
            .filter(|row| matches_exchange(row, &args.exchange))
            .filter_map(|row| {
                let exchange = text(row, "exchange")?;
                let oi = number(row, "open_interest")?;
                oi_change_pct(previous_oi, exchange, oi)
            })
            .max_by(f64::total_cmp);

        let coinglass_liquidation_value = external.as_ref().and_then(|value| {
            rows(value, "signals")
                .into_iter()
                .find_map(|row| number(row, "value"))
        });

        Ok(Self {
            min_funding_rate,
            max_oi_change_pct,
            spot_cvd_delta: latest_flow_delta(&spot_flow, &args.exchange),
            perp_cvd_delta: latest_flow_delta(&perp_flow, &args.exchange),
            upper_liquidation_notional: liquidation_notional_since(
                &liquidations,
                &args.exchange,
                15 * 60 * 1000,
            ),
            coinglass_liquidation_value,
            latest_price: latest_mid_price(&quotes, &args.exchange),
            reasons: Vec::new(),
        })
    }

    fn report(mut self) {
        let mut score = 0;

        match self.min_funding_rate {
            Some(rate) if rate <= -0.0005 => {
                score += 2;
                self.reasons
                    .push(format!("funding deeply negative: {:.4}%", rate * 100.0));
            }
            Some(rate) if rate < 0.0 => {
                score += 1;
                self.reasons.push(format!(
                    "funding negative but not extreme: {:.4}%",
                    rate * 100.0
                ));
            }
            Some(rate) => self
                .reasons
                .push(format!("funding not squeeze-biased: {:.4}%", rate * 100.0)),
            None => self.reasons.push("funding unavailable".to_string()),
        }

        match self.max_oi_change_pct {
            Some(change) if change >= 3.0 => {
                score += 2;
                self.reasons
                    .push(format!("OI expanded since last poll: +{change:.2}%"));
            }
            Some(change) if change > 0.0 => {
                score += 1;
                self.reasons
                    .push(format!("OI rising mildly: +{change:.2}%"));
            }
            Some(change) => self.reasons.push(format!("OI not expanding: {change:.2}%")),
            None => self
                .reasons
                .push("OI change needs at least two polling iterations".to_string()),
        }

        if self.spot_cvd_delta.is_some_and(|value| value > 0.0)
            && self.perp_cvd_delta.is_some_and(|value| value < 0.0)
        {
            score += 3;
            self.reasons.push(format!(
                "spot CVD up while perp CVD down: spot={:.0}, perp={:.0}",
                self.spot_cvd_delta.unwrap_or_default(),
                self.perp_cvd_delta.unwrap_or_default()
            ));
        } else {
            self.reasons.push(format!(
                "CVD divergence not confirmed: spot={:?}, perp={:?}",
                self.spot_cvd_delta, self.perp_cvd_delta
            ));
        }

        if self
            .upper_liquidation_notional
            .is_some_and(|value| value > 0.0)
        {
            score += 1;
            self.reasons.push(format!(
                "recent buy-side liquidation flow seen: {:.0} USDT",
                self.upper_liquidation_notional.unwrap_or_default()
            ));
        } else {
            self.reasons
                .push("native liquidation flow unavailable or quiet".to_string());
        }

        if let Some(value) = self.coinglass_liquidation_value {
            score += 1;
            self.reasons.push(format!(
                "CoinGlass aggregate liquidation metric present: {value:.2}"
            ));
        } else {
            self.reasons.push(
                "liquidation heatmap walls are not in the current schema; add a heatmap/custom source for 50x/100x walls".to_string(),
            );
        }

        if let Some(price) = self.latest_price {
            self.reasons.push(format!("latest mid price: {price:.4}"));
        }

        let verdict = match score {
            7..=9 => "strong squeeze setup",
            4..=6 => "watchlist",
            _ => "not enough confluence",
        };
        print_score(
            "Short squeeze long-follow setup",
            score,
            9,
            verdict,
            &self.reasons,
        );
    }
}

fn latest_flow_delta(value: &serde_json::Value, exchange: &Option<String>) -> Option<f64> {
    rows(value, "order_flow")
        .into_iter()
        .filter(|row| matches_exchange(row, exchange))
        .max_by_key(|row| ts_ms(row, "bucket_start_ms").unwrap_or_default())
        .and_then(|row| {
            number(row, "cumulative_delta_notional").or_else(|| number(row, "delta_notional"))
        })
}

fn liquidation_notional_since(
    value: &serde_json::Value,
    exchange: &Option<String>,
    lookback_ms: u64,
) -> Option<f64> {
    let now = marketbridge_now_ms();
    let total = rows(value, "liquidations")
        .into_iter()
        .filter(|row| matches_exchange(row, exchange))
        .filter(|row| ts_ms(row, "ts_ms").is_none_or(|ts| now.saturating_sub(ts) <= lookback_ms))
        .filter_map(signed_liquidation_notional)
        .filter(|value| *value > 0.0)
        .sum::<f64>();
    (total > 0.0).then_some(total)
}

fn latest_mid_price(value: &serde_json::Value, exchange: &Option<String>) -> Option<f64> {
    rows(value, "quotes")
        .into_iter()
        .filter(|row| matches_exchange(row, exchange))
        .filter_map(|row| {
            let bid = row
                .pointer("/payload/bid")
                .and_then(serde_json::Value::as_f64)?;
            let ask = row
                .pointer("/payload/ask")
                .and_then(serde_json::Value::as_f64)?;
            Some((
                (bid + ask) / 2.0,
                row.pointer("/freshness/ts_received")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default(),
            ))
        })
        .max_by_key(|(_, ts)| *ts)
        .map(|(mid, _)| mid)
}

fn marketbridge_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
