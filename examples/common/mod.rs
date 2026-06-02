#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Args {
    pub base_url: String,
    pub symbol: String,
    pub exchange: Option<String>,
    pub interval_secs: u64,
    pub iterations: Option<usize>,
}

impl Args {
    pub fn parse() -> Self {
        let mut args = std::env::args().skip(1);
        let mut out = Self {
            base_url: "http://127.0.0.1:8080".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: None,
            interval_secs: 30,
            iterations: None,
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--base-url" => {
                    if let Some(value) = args.next() {
                        out.base_url = value.trim_end_matches('/').to_string();
                    }
                }
                "--symbol" => {
                    if let Some(value) = args.next() {
                        out.symbol = value.to_ascii_uppercase();
                    }
                }
                "--exchange" => {
                    if let Some(value) = args.next() {
                        out.exchange = Some(value.to_ascii_lowercase());
                    }
                }
                "--interval-secs" => {
                    if let Some(value) = args.next().and_then(|value| value.parse().ok()) {
                        out.interval_secs = value;
                    }
                }
                "--iterations" => {
                    if let Some(value) = args.next().and_then(|value| value.parse().ok()) {
                        out.iterations = Some(value);
                    }
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {}
            }
        }

        out
    }
}

fn print_help() {
    println!(
        "Usage: cargo run --example <name> -- --symbol BTCUSDT [--exchange binance] [--base-url http://127.0.0.1:8080] [--interval-secs 30] [--iterations 10]"
    );
}

pub async fn get_json(client: &Client, base_url: &str, path: &str) -> Result<Value> {
    let url = format!("{base_url}{path}");
    client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("non-success response: {url}"))?
        .json::<Value>()
        .await
        .with_context(|| format!("invalid json: {url}"))
}

pub fn rows<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

pub fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|value| match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

pub fn ts_ms(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|value| match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

pub fn matches_exchange(row: &Value, exchange: &Option<String>) -> bool {
    exchange.as_ref().is_none_or(|target| {
        text(row, "exchange").is_some_and(|value| value.eq_ignore_ascii_case(target))
            || row
                .pointer("/source_ref/source")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(target))
    })
}

pub fn marketbridge_symbol_to_aggregate_symbol(symbol: &str) -> String {
    for suffix in ["USDT", "USDC", "USD", "PERP"] {
        if let Some(base) = symbol.strip_suffix(suffix) {
            return base.to_string();
        }
    }
    symbol.to_string()
}

pub fn side(row: &Value) -> Option<&str> {
    text(row, "side")
}

pub fn signed_liquidation_notional(row: &Value) -> Option<f64> {
    let notional = number(row, "price")? * number(row, "qty")?;
    match side(row).map(str::to_ascii_lowercase).as_deref() {
        Some("buy") => Some(notional),
        Some("sell") => Some(-notional),
        _ => Some(0.0),
    }
}

pub fn print_score(title: &str, score: i32, max_score: i32, verdict: &str, reasons: &[String]) {
    println!("\n{title}");
    println!("score: {score}/{max_score} | verdict: {verdict}");
    for reason in reasons {
        println!("- {reason}");
    }
}

pub fn sleep_duration(args: &Args) -> Duration {
    Duration::from_secs(args.interval_secs.max(1))
}

pub fn iteration_done(iteration: usize, args: &Args) -> bool {
    args.iterations.is_some_and(|limit| iteration + 1 >= limit)
}

pub fn oi_change_pct(
    previous_oi: &mut HashMap<String, f64>,
    exchange: &str,
    current: f64,
) -> Option<f64> {
    let old = previous_oi.insert(exchange.to_string(), current)?;
    (old > 0.0).then_some((current - old) / old * 100.0)
}
