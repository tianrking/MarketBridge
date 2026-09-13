mod common;

use anyhow::Result;
use common::{
    Args, get_json, iteration_done, matches_exchange, number, print_score, rows, sleep_duration,
    text,
};
use reqwest::Client;

/// Research-only spot/perpetual carry monitor.
///
/// This is deliberately a signal-quality example, not an execution recipe:
/// basis on a perpetual has no fixed convergence date, funding intervals vary
/// by venue, and borrow, margin, fees and slippage are not inferred here.
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::new();

    println!(
        "basis carry monitor | symbol={} exchange={}",
        args.symbol,
        args.exchange.as_deref().unwrap_or("all")
    );

    for iteration in 0usize.. {
        let basis = get_json(
            &client,
            &args.base_url,
            &format!("/v1/market/basis?symbols={}", args.symbol),
        )
        .await?;
        let funding = get_json(
            &client,
            &args.base_url,
            &format!("/v1/market/funding?symbols={}", args.symbol),
        )
        .await?;
        report(&args, &basis, &funding);

        if iteration_done(iteration, &args) {
            break;
        }
        tokio::time::sleep(sleep_duration(&args)).await;
    }

    Ok(())
}

fn report(args: &Args, basis: &serde_json::Value, funding: &serde_json::Value) {
    let mut score = 0;
    let mut reasons = Vec::new();

    let basis_row = rows(basis, "basis")
        .into_iter()
        .filter(|row| {
            text(row, "symbol").is_some_and(|value| value.eq_ignore_ascii_case(&args.symbol))
        })
        .filter(|row| matches_exchange(row, &args.exchange))
        .max_by_key(|row| number(row, "spot_ts_ms").unwrap_or_default() as u64);
    let funding_row = rows(funding, "funding")
        .into_iter()
        .filter(|row| {
            text(row, "symbol").is_some_and(|value| value.eq_ignore_ascii_case(&args.symbol))
        })
        .filter(|row| matches_exchange(row, &args.exchange))
        .max_by_key(|row| number(row, "ts_ms").unwrap_or_default() as u64);

    match (basis_row, funding_row) {
        (Some(basis), Some(funding)) => {
            let basis_bps = number(basis, "basis_bps");
            let funding_rate = number(funding, "funding_rate");
            let funding_interval_ms = number(funding, "funding_interval_ms");
            let stale = basis
                .get("stale")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);

            if stale {
                reasons.push("basis snapshot is stale; candidate is rejected".to_string());
            } else if let Some(value) = basis_bps {
                if value >= 10.0 {
                    score += 2;
                    reasons.push(format!("perp trades {value:.2} bps above spot"));
                } else if value > 0.0 {
                    score += 1;
                    reasons.push(format!("positive perp basis: {value:.2} bps"));
                } else {
                    reasons.push(format!("basis is not positive: {value:.2} bps"));
                }
            } else {
                reasons.push("basis_bps unavailable".to_string());
            }

            if let (Some(rate), Some(interval_ms)) = (funding_rate, funding_interval_ms) {
                let periods_per_day = 86_400_000.0 / interval_ms.max(1.0);
                let daily_bps = rate * periods_per_day * 10_000.0;
                if rate > 0.0 {
                    score += 2;
                    reasons.push(format!(
                        "short-perp funding receipt proxy: {daily_bps:.2} bps/day"
                    ));
                } else {
                    reasons.push(format!("short-perp pays funding: {daily_bps:.2} bps/day"));
                }
            } else if funding_rate.is_some() {
                reasons.push(
                    "funding rate available but settlement interval is unknown; annualization withheld"
                        .to_string(),
                );
            } else {
                reasons.push("funding unavailable".to_string());
            }

            if let (Some(basis_bps), Some(rate), Some(interval_ms)) =
                (basis_bps, funding_rate, funding_interval_ms)
            {
                let daily_funding_bps = rate * (86_400_000.0 / interval_ms.max(1.0)) * 10_000.0;
                let verdict = if !stale && basis_bps >= 10.0 && daily_funding_bps > 0.0 {
                    "research candidate: positive carry inputs"
                } else {
                    "observe only: carry inputs incomplete"
                };
                print_score(
                    "Spot/perpetual carry hypothesis",
                    score,
                    4,
                    verdict,
                    &reasons,
                );
            } else {
                print_score(
                    "Spot/perpetual carry hypothesis",
                    score,
                    4,
                    "insufficient evidence",
                    &reasons,
                );
            }
        }
        _ => print_score(
            "Spot/perpetual carry hypothesis",
            0,
            4,
            "insufficient evidence",
            &["matching basis and funding rows were not available".to_string()],
        ),
    }
}
