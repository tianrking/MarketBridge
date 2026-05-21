use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::common::{
    OptionBook, OptionSummary, option_side_from_code, parse_day_month_year_expiry, parse_f64_opt,
};
use crate::types::BookLevel;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BybitResponse<T> {
    ret_code: i64,
    ret_msg: String,
    result: BybitResult<T>,
}

#[derive(Debug, Deserialize)]
struct BybitResult<T> {
    list: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BybitDirectResponse<T> {
    ret_code: i64,
    ret_msg: String,
    result: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BybitOptionTicker {
    symbol: String,
    #[serde(default)]
    bid1_price: Option<String>,
    #[serde(default)]
    ask1_price: Option<String>,
    #[serde(default)]
    mark_price: Option<String>,
    #[serde(default)]
    mark_iv: Option<String>,
    #[serde(default)]
    underlying_price: Option<String>,
    #[serde(default)]
    index_price: Option<String>,
    #[serde(default)]
    open_interest: Option<String>,
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
struct BybitOrderBookResult {
    s: String,
    #[serde(default)]
    b: Vec<[String; 2]>,
    #[serde(default)]
    a: Vec<[String; 2]>,
    #[serde(default)]
    u: Option<u64>,
    #[serde(default)]
    ts: Option<u64>,
}

pub async fn fetch_bybit_option_summaries_from(
    client: &reqwest::Client,
    base_url: &str,
    currency: &str,
) -> Result<Vec<OptionSummary>> {
    let currency = currency.trim().to_ascii_uppercase();
    let mut url = Url::parse(base_url)?.join("market/tickers")?;
    url.query_pairs_mut()
        .append_pair("category", "option")
        .append_pair("baseCoin", &currency);

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<BybitResponse<Vec<BybitOptionTicker>>>()
        .await
        .context("failed to decode bybit option tickers")?;

    if response.ret_code != 0 {
        anyhow::bail!("bybit option tickers failed: {}", response.ret_msg);
    }

    Ok(response
        .result
        .list
        .into_iter()
        .map(|raw| {
            let parsed = parse_bybit_instrument(&raw.symbol);
            OptionSummary {
                venue: "bybit".to_string(),
                currency: currency.clone(),
                instrument_name: raw.symbol,
                option_type: parsed.as_ref().map(|p| p.2.clone()),
                strike: parsed.as_ref().map(|p| p.1),
                expiry_time: parsed.map(|p| p.0),
                bid_price: parse_f64_opt(raw.bid1_price.as_deref()),
                ask_price: parse_f64_opt(raw.ask1_price.as_deref()),
                mark_price: parse_f64_opt(raw.mark_price.as_deref()),
                mark_iv: parse_f64_opt(raw.mark_iv.as_deref()),
                delta: parse_f64_opt(raw.delta.as_deref()),
                gamma: parse_f64_opt(raw.gamma.as_deref()),
                theta: parse_f64_opt(raw.theta.as_deref()),
                vega: parse_f64_opt(raw.vega.as_deref()),
                underlying_price: parse_f64_opt(raw.underlying_price.as_deref())
                    .or_else(|| parse_f64_opt(raw.index_price.as_deref())),
                underlying_index: Some(currency.clone()),
                open_interest: parse_f64_opt(raw.open_interest.as_deref()),
            }
        })
        .collect())
}

pub async fn fetch_bybit_option_book_from(
    client: &reqwest::Client,
    base_url: &str,
    instrument_name: &str,
    depth: usize,
) -> Result<OptionBook> {
    let mut url = Url::parse(base_url)?.join("market/orderbook")?;
    url.query_pairs_mut()
        .append_pair("category", "option")
        .append_pair("symbol", instrument_name)
        .append_pair("limit", &depth.clamp(1, 200).to_string());

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<BybitDirectResponse<BybitOrderBookResult>>()
        .await
        .context("failed to decode bybit option order book")?;

    if response.ret_code != 0 {
        anyhow::bail!("bybit option order book failed: {}", response.ret_msg);
    }

    Ok(response.result.into_book())
}

impl BybitOrderBookResult {
    fn into_book(self) -> OptionBook {
        let bids = bybit_levels(self.b);
        let asks = bybit_levels(self.a);
        OptionBook {
            venue: "bybit".to_string(),
            instrument_name: self.s,
            timestamp: self.ts,
            state: self.u.map(|value| format!("update_id:{value}")),
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

fn bybit_levels(rows: Vec<[String; 2]>) -> Vec<BookLevel> {
    rows.into_iter()
        .filter_map(|row| {
            Some(BookLevel {
                price: parse_f64_opt(Some(row[0].as_str()))?,
                qty: parse_f64_opt(Some(row[1].as_str()))?,
            })
        })
        .collect()
}

fn parse_bybit_instrument(name: &str) -> Option<(String, f64, String)> {
    let parts = name.split('-').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    let expiry_time = parse_day_month_year_expiry(parts[1])?;
    let strike = parts[2].parse::<f64>().ok()?;
    let option_type = option_side_from_code(parts[3]);
    Some((expiry_time, strike, option_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bybit_option_symbol() {
        let parsed = parse_bybit_instrument("BTC-26MAR27-50000-C-USDT").unwrap();
        assert_eq!(parsed.0, "2027-03-26T08:00:00Z");
        assert_eq!(parsed.1, 50_000.0);
        assert_eq!(parsed.2, "call");
    }

    #[test]
    fn bybit_option_book_extracts_depth() {
        let book = BybitOrderBookResult {
            s: "BTC-26MAR27-78000-P-USDT".to_string(),
            b: vec![["10745".to_string(), "23.63".to_string()]],
            a: vec![["13140".to_string(), "23.63".to_string()]],
            u: Some(66108403357),
            ts: Some(1779334083557),
        }
        .into_book();
        assert_eq!(book.bid_price, Some(10745.0));
        assert_eq!(book.ask_price, Some(13140.0));
        assert_eq!(book.bids[0].qty, 23.63);
    }
}
