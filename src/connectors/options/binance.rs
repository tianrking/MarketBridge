use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::common::{
    OptionBook, OptionSummary, option_side_from_code, parse_f64_opt, parse_yy_mm_dd_expiry,
};
use crate::types::BookLevel;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOptionTicker {
    symbol: String,
    #[serde(default)]
    bid_price: Option<String>,
    #[serde(default)]
    ask_price: Option<String>,
    #[serde(default)]
    last_price: Option<String>,
    #[serde(default)]
    strike_price: Option<String>,
    #[serde(default)]
    exercise_price: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOptionMark {
    symbol: String,
    #[serde(default)]
    mark_price: Option<String>,
    #[serde(default)]
    mark_iv: Option<String>,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    gamma: Option<String>,
    #[serde(default)]
    theta: Option<String>,
    #[serde(default)]
    vega: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOptionDepth {
    #[serde(default)]
    bids: Vec<[String; 2]>,
    #[serde(default)]
    asks: Vec<[String; 2]>,
    #[serde(default)]
    last_update_id: Option<u64>,
    #[serde(default, rename = "T")]
    time: Option<u64>,
}

pub async fn fetch_binance_option_summaries_from(
    client: &reqwest::Client,
    base_url: &str,
    currency: &str,
) -> Result<Vec<OptionSummary>> {
    let currency = currency.trim().to_ascii_uppercase();
    let ticker_url = Url::parse(base_url)?.join("eapi/v1/ticker")?;
    let mark_url = Url::parse(base_url)?.join("eapi/v1/mark")?;

    let tickers = client
        .get(ticker_url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<BinanceOptionTicker>>()
        .await
        .context("failed to decode binance option tickers")?;

    let marks = fetch_marks(client, mark_url).await;

    Ok(tickers
        .into_iter()
        .filter(|raw| raw.symbol.starts_with(&format!("{currency}-")))
        .map(|raw| {
            let parsed = parse_binance_instrument(&raw.symbol);
            let mark = marks.get(&raw.symbol);
            OptionSummary {
                venue: "binance".to_string(),
                currency: currency.clone(),
                instrument_name: raw.symbol,
                option_type: parsed.as_ref().map(|p| p.2.clone()),
                strike: parse_f64_opt(raw.strike_price.as_deref())
                    .or_else(|| parsed.as_ref().map(|p| p.1)),
                expiry_time: parsed.map(|p| p.0),
                bid_price: parse_f64_opt(raw.bid_price.as_deref()),
                ask_price: parse_f64_opt(raw.ask_price.as_deref()),
                mark_price: mark
                    .and_then(|row| parse_f64_opt(row.mark_price.as_deref()))
                    .or_else(|| parse_f64_opt(raw.last_price.as_deref())),
                mark_iv: mark.and_then(|row| parse_f64_opt(row.mark_iv.as_deref())),
                delta: mark.and_then(|row| parse_f64_opt(row.delta.as_deref())),
                gamma: mark.and_then(|row| parse_f64_opt(row.gamma.as_deref())),
                theta: mark.and_then(|row| parse_f64_opt(row.theta.as_deref())),
                vega: mark.and_then(|row| parse_f64_opt(row.vega.as_deref())),
                underlying_price: parse_f64_opt(raw.exercise_price.as_deref()),
                underlying_index: Some(currency.clone()),
                open_interest: None,
            }
        })
        .collect())
}

pub async fn fetch_binance_option_book_from(
    client: &reqwest::Client,
    base_url: &str,
    instrument_name: &str,
    depth: usize,
) -> Result<OptionBook> {
    let mut url = Url::parse(base_url)?.join("eapi/v1/depth")?;
    url.query_pairs_mut()
        .append_pair("symbol", instrument_name)
        .append_pair("limit", &binance_depth_limit(depth).to_string());

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<BinanceOptionDepth>()
        .await
        .context("failed to decode binance option order book")?;

    Ok(response.into_book(instrument_name))
}

impl BinanceOptionDepth {
    fn into_book(self, instrument_name: &str) -> OptionBook {
        let bids = binance_levels(self.bids);
        let asks = binance_levels(self.asks);
        OptionBook {
            venue: "binance".to_string(),
            instrument_name: instrument_name.to_string(),
            timestamp: self.time,
            state: self
                .last_update_id
                .map(|value| format!("last_update_id:{value}")),
            bid_price: bids.first().map(|level| level.price),
            ask_price: asks.first().map(|level| level.price),
            mark_price: None,
            mark_iv: None,
            underlying_price: None,
            underlying_index: None,
            open_interest: None,
            delta: None,
            gamma: None,
            theta: None,
            vega: None,
            bids,
            asks,
        }
    }
}

fn binance_depth_limit(depth: usize) -> usize {
    match depth {
        0..=10 => 10,
        11..=20 => 20,
        21..=50 => 50,
        51..=100 => 100,
        _ => 1000,
    }
}

fn binance_levels(rows: Vec<[String; 2]>) -> Vec<BookLevel> {
    rows.into_iter()
        .filter_map(|row| {
            Some(BookLevel {
                price: parse_f64_opt(Some(row[0].as_str()))?,
                qty: parse_f64_opt(Some(row[1].as_str()))?,
            })
        })
        .collect()
}

async fn fetch_marks(client: &reqwest::Client, url: Url) -> HashMap<String, BinanceOptionMark> {
    match client.get(url).send().await {
        Ok(response) => match response.error_for_status() {
            Ok(ok) => ok
                .json::<Vec<BinanceOptionMark>>()
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|row| (row.symbol.clone(), row))
                .collect(),
            Err(_) => HashMap::new(),
        },
        Err(_) => HashMap::new(),
    }
}

fn parse_binance_instrument(name: &str) -> Option<(String, f64, String)> {
    let parts = name.split('-').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    let expiry_time = parse_yy_mm_dd_expiry(parts[1])?;
    let strike = parts[2].parse::<f64>().ok()?;
    let option_type = option_side_from_code(parts[3]);
    Some((expiry_time, strike, option_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binance_option_symbol() {
        let parsed = parse_binance_instrument("BTC-260626-140000-C").unwrap();
        assert_eq!(parsed.0, "2026-06-26T08:00:00Z");
        assert_eq!(parsed.1, 140_000.0);
        assert_eq!(parsed.2, "call");
    }

    #[test]
    fn binance_option_book_extracts_depth() {
        let book = BinanceOptionDepth {
            bids: vec![["5.000".to_string(), "0.21".to_string()]],
            asks: vec![["15.000".to_string(), "1.10".to_string()]],
            last_update_id: Some(18803530158),
            time: Some(1779334083503),
        }
        .into_book("BTC-260626-140000-C");
        assert_eq!(book.bid_price, Some(5.0));
        assert_eq!(book.ask_price, Some(15.0));
        assert_eq!(book.asks[0].qty, 1.10);
        assert_eq!(binance_depth_limit(5), 10);
        assert_eq!(binance_depth_limit(51), 100);
    }
}
