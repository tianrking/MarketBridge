mod common;

use std::collections::HashMap;

use anyhow::Result;
use common::{
    Args, get_json, iteration_done, matches_exchange, number, oi_change_pct, print_score, rows,
    sleep_duration, text, ts_ms,
};
use reqwest::Client;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::new();
    let mut previous_oi = HashMap::new();

    println!(
        "exhaustion short monitor | symbol={} exchange={}",
        args.symbol,
        args.exchange.as_deref().unwrap_or("all")
    );

    for iteration in 0usize.. {
        let snapshot = Snapshot::load(&client, &args, &mut previous_oi).await?;
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
    max_funding_rate: Option<f64>,
    max_oi_drop_pct: Option<f64>,
    recent_return_pct: Option<f64>,
    recent_high_failure: Option<bool>,
    perp_cvd_delta: Option<f64>,
    bid_ask_depth_ratio: Option<f64>,
    cex_inflow_notional: Option<f64>,
    reasons: Vec<String>,
}

impl Snapshot {
    async fn load(
        client: &Client,
        args: &Args,
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
        let perp_flow = get_json(
            client,
            &args.base_url,
            &format!(
                "/v1/market/order-flow?market=perp&symbol={}&window_ms=60000&limit=50",
                args.symbol
            ),
        )
        .await?;
        let books = get_json(
            client,
            &args.base_url,
            &format!("/v1/market/order-books?market=perp&symbols={}", args.symbol),
        )
        .await?;
        let klines = load_klines(client, args).await.ok();
        let transfers = get_json(
            client,
            &args.base_url,
            "/v1/onchain/transfers?min_amount_usd=1000000",
        )
        .await
        .ok();

        let mut out = Self::default();
        out.max_funding_rate = rows(&funding, "funding")
            .into_iter()
            .filter(|row| matches_exchange(row, &args.exchange))
            .filter_map(|row| number(row, "funding_rate"))
            .max_by(f64::total_cmp);

        out.max_oi_drop_pct = rows(&open_interest, "open_interest")
            .into_iter()
            .filter(|row| matches_exchange(row, &args.exchange))
            .filter_map(|row| {
                let exchange = text(row, "exchange")?;
                let oi = number(row, "open_interest")?;
                oi_change_pct(previous_oi, exchange, oi)
            })
            .min_by(f64::total_cmp);

        if let Some(klines) = klines.as_ref() {
            let (ret, failed_high) = recent_price_context(klines);
            out.recent_return_pct = ret;
            out.recent_high_failure = failed_high;
        }

        out.perp_cvd_delta = latest_flow_delta(&perp_flow, &args.exchange);
        out.bid_ask_depth_ratio = weakest_bid_ask_ratio(&books, &args.exchange);
        out.cex_inflow_notional = transfers
            .as_ref()
            .and_then(|value| cex_inflow_notional(value, &args.symbol));

        Ok(out)
    }

    fn report(mut self) {
        let mut score = 0;

        match self.max_funding_rate {
            Some(rate) if rate >= 0.0005 => {
                score += 2;
                self.reasons
                    .push(format!("funding extremely positive: {:.4}%", rate * 100.0));
            }
            Some(rate) if rate > 0.0 => {
                score += 1;
                self.reasons
                    .push(format!("funding positive: {:.4}%", rate * 100.0));
            }
            Some(rate) => self
                .reasons
                .push(format!("funding not long-crowded: {:.4}%", rate * 100.0)),
            None => self.reasons.push("funding unavailable".to_string()),
        }

        if self.recent_high_failure == Some(true) {
            score += 2;
            self.reasons.push(format!(
                "recent candles show push up then failure: return={:?}%",
                self.recent_return_pct
                    .map(|value| (value * 100.0).round() / 100.0)
            ));
        } else {
            self.reasons.push(format!(
                "no clear high-failure pattern from local klines: return={:?}%",
                self.recent_return_pct
                    .map(|value| (value * 100.0).round() / 100.0)
            ));
        }

        match self.max_oi_drop_pct {
            Some(change) if change <= -3.0 => {
                score += 2;
                self.reasons
                    .push(format!("OI dropped since last poll: {change:.2}%"));
            }
            Some(change) if change < 0.0 => {
                score += 1;
                self.reasons.push(format!("OI slipping: {change:.2}%"));
            }
            Some(change) => self.reasons.push(format!("OI not falling: {change:.2}%")),
            None => self
                .reasons
                .push("OI change needs at least two polling iterations".to_string()),
        }

        if self.perp_cvd_delta.is_some_and(|value| value < 0.0) {
            score += 1;
            self.reasons.push(format!(
                "perp CVD is sell-biased: {:.0}",
                self.perp_cvd_delta.unwrap_or_default()
            ));
        } else {
            self.reasons.push(format!(
                "perp CVD not sell-biased: {:?}",
                self.perp_cvd_delta
            ));
        }

        match self.bid_ask_depth_ratio {
            Some(ratio) if ratio < 0.65 => {
                score += 2;
                self.reasons.push(format!(
                    "bid wall is thin versus asks: bid/ask depth ratio={ratio:.2}"
                ));
            }
            Some(ratio) if ratio < 1.0 => {
                score += 1;
                self.reasons.push(format!(
                    "bids weaker than asks: bid/ask depth ratio={ratio:.2}"
                ));
            }
            Some(ratio) => self.reasons.push(format!(
                "book still has support: bid/ask depth ratio={ratio:.2}"
            )),
            None => self.reasons.push("perp order book unavailable".to_string()),
        }

        if self
            .cex_inflow_notional
            .is_some_and(|value| value >= 1_000_000.0)
        {
            score += 1;
            self.reasons.push(format!(
                "large on-chain transfer context present: {:.0} USD",
                self.cex_inflow_notional.unwrap_or_default()
            ));
        } else {
            self.reasons.push(
                "CEX inflow requires Whale Alert/Etherscan configuration and exchange address tagging".to_string(),
            );
        }

        let verdict = match score {
            8..=10 => "strong exhaustion short setup",
            5..=7 => "watchlist",
            _ => "not enough confluence",
        };
        print_score(
            "Exhaustion / distribution short setup",
            score,
            10,
            verdict,
            &self.reasons,
        );
    }
}

async fn load_klines(client: &Client, args: &Args) -> Result<Value> {
    let exchange = args.exchange.as_deref().unwrap_or("binance");
    get_json(
        client,
        &args.base_url,
        &format!(
            "/v1/market/klines?exchange={exchange}&market=perp&symbol={}&interval=1m&limit=30",
            args.symbol
        ),
    )
    .await
}

fn recent_price_context(value: &Value) -> (Option<f64>, Option<bool>) {
    let bars = rows(value, "klines");
    let first = bars.first().and_then(|row| number(row, "open"));
    let last_close = bars.last().and_then(|row| number(row, "close"));
    let recent_high = bars
        .iter()
        .rev()
        .take(10)
        .filter_map(|row| number(row, "high"))
        .max_by(f64::total_cmp);
    let previous_high = bars
        .iter()
        .rev()
        .skip(10)
        .filter_map(|row| number(row, "high"))
        .max_by(f64::total_cmp);
    let ret = first
        .zip(last_close)
        .and_then(|(first, last)| (first > 0.0).then_some((last - first) / first * 100.0));
    let failed_high = recent_high
        .zip(previous_high)
        .zip(last_close)
        .map(|((recent, previous), close)| recent > previous && close < recent * 0.997);
    (ret, failed_high)
}

fn latest_flow_delta(value: &Value, exchange: &Option<String>) -> Option<f64> {
    rows(value, "order_flow")
        .into_iter()
        .filter(|row| matches_exchange(row, exchange))
        .max_by_key(|row| ts_ms(row, "bucket_start_ms").unwrap_or_default())
        .and_then(|row| {
            number(row, "cumulative_delta_notional").or_else(|| number(row, "delta_notional"))
        })
}

fn weakest_bid_ask_ratio(value: &Value, exchange: &Option<String>) -> Option<f64> {
    rows(value, "books")
        .into_iter()
        .filter(|row| matches_exchange(row, exchange))
        .filter_map(|row| {
            let bid_depth = row
                .get("bids")?
                .as_array()?
                .iter()
                .take(10)
                .filter_map(level_notional)
                .sum::<f64>();
            let ask_depth = row
                .get("asks")?
                .as_array()?
                .iter()
                .take(10)
                .filter_map(level_notional)
                .sum::<f64>();
            (ask_depth > 0.0).then_some(bid_depth / ask_depth)
        })
        .min_by(f64::total_cmp)
}

fn level_notional(level: &Value) -> Option<f64> {
    Some(number(level, "price")? * number(level, "qty")?)
}

fn cex_inflow_notional(value: &Value, symbol: &str) -> Option<f64> {
    let base = symbol
        .trim_end_matches("USDT")
        .trim_end_matches("USDC")
        .trim_end_matches("USD")
        .to_ascii_uppercase();
    let total = rows(value, "transfers")
        .into_iter()
        .filter(|row| {
            text(row, "asset")
                .or_else(|| text(row, "symbol"))
                .is_none_or(|asset| asset.eq_ignore_ascii_case(&base))
        })
        .filter(|row| {
            text(row, "to_label")
                .or_else(|| text(row, "to_owner"))
                .or_else(|| text(row, "to"))
                .is_some_and(|to| {
                    let to = to.to_ascii_lowercase();
                    [
                        "binance", "coinbase", "okx", "bybit", "kraken", "kucoin", "gate",
                    ]
                    .iter()
                    .any(|name| to.contains(name))
                })
        })
        .filter_map(|row| number(row, "amount_usd").or_else(|| number(row, "value_usd")))
        .sum::<f64>();
    (total > 0.0).then_some(total)
}
