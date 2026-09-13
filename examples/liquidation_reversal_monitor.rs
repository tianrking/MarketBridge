mod common;

use anyhow::Result;
use common::{
    Args, get_json, iteration_done, matches_exchange, number, print_score, rows, sleep_duration,
    text,
};
use reqwest::Client;
use serde_json::Value;

/// Research-only liquidation-flush / mean-reversion observer.
///
/// It looks for sell-side liquidations (a proxy for long liquidation), a
/// falling OI baseline, a positive recent candle return and positive perp CVD.
/// It is intentionally a candidate report, not an entry or execution rule.
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::new();
    let mut previous_oi = std::collections::HashMap::new();

    println!(
        "liquidation reversal monitor | symbol={} exchange={}",
        args.symbol,
        args.exchange.as_deref().unwrap_or("all")
    );

    for iteration in 0usize.. {
        let open_interest = get_json(
            &client,
            &args.base_url,
            &format!("/v1/market/open-interest?symbols={}", args.symbol),
        )
        .await?;
        let liquidations = get_json(
            &client,
            &args.base_url,
            &format!("/v1/market/liquidations?symbols={}", args.symbol),
        )
        .await?;
        let flow = get_json(
            &client,
            &args.base_url,
            &format!(
                "/v1/market/order-flow?market=perp&symbol={}&window_ms=900000&limit=50",
                args.symbol
            ),
        )
        .await?;
        let klines = get_json(
            &client,
            &args.base_url,
            &format!(
                "/v1/market/klines?exchange={}&market=perp&symbol={}&interval=5m&limit=12",
                args.exchange.as_deref().unwrap_or("binance"),
                args.symbol
            ),
        )
        .await
        .ok();

        report(
            &args,
            &open_interest,
            &liquidations,
            &flow,
            klines.as_ref(),
            &mut previous_oi,
        );

        if iteration_done(iteration, &args) {
            break;
        }
        tokio::time::sleep(sleep_duration(&args)).await;
    }

    Ok(())
}

fn report(
    args: &Args,
    open_interest: &Value,
    liquidations: &Value,
    flow: &Value,
    klines: Option<&Value>,
    previous_oi: &mut std::collections::HashMap<String, f64>,
) {
    let mut score = 0;
    let mut reasons = Vec::new();

    let oi_change = rows(open_interest, "open_interest")
        .into_iter()
        .filter(|row| matches_exchange(row, &args.exchange))
        .filter_map(|row| {
            let exchange = text(row, "exchange")?;
            let current = number(row, "open_interest")?;
            let previous = previous_oi.insert(exchange.to_string(), current)?;
            (previous > 0.0).then_some((current - previous) / previous * 100.0)
        })
        .min_by(f64::total_cmp);

    match oi_change {
        Some(change) if change <= -3.0 => {
            score += 2;
            reasons.push(format!("OI fell after the flush: {change:.2}%"));
        }
        Some(change) if change < 0.0 => {
            score += 1;
            reasons.push(format!("OI is easing: {change:.2}%"));
        }
        Some(change) => reasons.push(format!("OI did not fall: {change:.2}%")),
        None => reasons.push("OI needs at least two observations".to_string()),
    }

    let sell_liquidation_notional: f64 = rows(liquidations, "liquidations")
        .into_iter()
        .filter(|row| matches_exchange(row, &args.exchange))
        .filter(|row| text(row, "symbol").is_none_or(|s| s.eq_ignore_ascii_case(&args.symbol)))
        .filter(|row| text(row, "side").is_some_and(|side| side.eq_ignore_ascii_case("sell")))
        .filter_map(|row| Some(number(row, "price")? * number(row, "qty")?))
        .sum();
    if sell_liquidation_notional > 0.0 {
        score += 1;
        reasons.push(format!(
            "sell-side liquidation notional observed: {sell_liquidation_notional:.0}"
        ));
    } else {
        reasons.push("no sell-side liquidation evidence in the current cache".to_string());
    }

    let cvd = rows(flow, "order_flow")
        .into_iter()
        .filter(|row| matches_exchange(row, &args.exchange))
        .max_by_key(|row| number(row, "bucket_start_ms").unwrap_or_default() as u64)
        .and_then(|row| {
            number(row, "cumulative_delta_notional").or_else(|| number(row, "delta_notional"))
        });
    if cvd.is_some_and(|value| value > 0.0) {
        score += 1;
        reasons.push(format!(
            "perp CVD turned positive: {:.0}",
            cvd.unwrap_or_default()
        ));
    } else {
        reasons.push(format!("perp CVD is not positive: {cvd:?}"));
    }

    let recent_return_pct = klines.and_then(recent_return_pct);
    if recent_return_pct.is_some_and(|value| value > 0.0) {
        score += 1;
        reasons.push(format!(
            "recent 5m candles recovered: {:.2}%",
            recent_return_pct.unwrap_or_default()
        ));
    } else {
        reasons.push(format!(
            "price recovery not confirmed: {recent_return_pct:?}%"
        ));
    }

    let verdict = match score {
        5 => "research candidate: flush plus recovery confluence",
        3..=4 => "watchlist: partial reversal evidence",
        _ => "observe only: insufficient evidence",
    };
    print_score(
        "Liquidation-flush reversal hypothesis",
        score,
        5,
        verdict,
        &reasons,
    );
}

fn recent_return_pct(value: &Value) -> Option<f64> {
    let bars = rows(value, "klines");
    let open = bars.first().and_then(|row| number(row, "open"))?;
    let close = bars.last().and_then(|row| number(row, "close"))?;
    (open > 0.0).then_some((close - open) / open * 100.0)
}
