use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::common::{OptionSummary, option_side_from_code, parse_day_month_year_expiry};
use crate::types::BookLevel;

#[derive(Debug, Deserialize)]
struct DeribitResponse<T> {
    result: T,
}

#[derive(Debug, Deserialize)]
struct DeribitRawSummary {
    instrument_name: String,
    bid_price: Option<f64>,
    ask_price: Option<f64>,
    mark_price: Option<f64>,
    mark_iv: Option<f64>,
    underlying_price: Option<f64>,
    underlying_index: Option<String>,
    open_interest: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeribitOptionBook {
    pub venue: String,
    pub instrument_name: String,
    pub timestamp: Option<u64>,
    pub state: Option<String>,
    pub bid_price: Option<f64>,
    pub ask_price: Option<f64>,
    pub mark_price: Option<f64>,
    pub mark_iv: Option<f64>,
    pub underlying_price: Option<f64>,
    pub underlying_index: Option<String>,
    pub open_interest: Option<f64>,
    pub delta: Option<f64>,
    pub gamma: Option<f64>,
    pub theta: Option<f64>,
    pub vega: Option<f64>,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
}

#[derive(Debug, Deserialize)]
struct DeribitRawBookResponse {
    result: DeribitRawBook,
}

#[derive(Debug, Deserialize)]
struct DeribitRawBook {
    instrument_name: String,
    timestamp: Option<u64>,
    state: Option<String>,
    best_bid_price: Option<f64>,
    best_ask_price: Option<f64>,
    mark_price: Option<f64>,
    mark_iv: Option<f64>,
    underlying_price: Option<f64>,
    underlying_index: Option<String>,
    open_interest: Option<f64>,
    greeks: Option<DeribitGreeks>,
    #[serde(default)]
    bids: Vec<Vec<f64>>,
    #[serde(default)]
    asks: Vec<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct DeribitGreeks {
    delta: Option<f64>,
    gamma: Option<f64>,
    theta: Option<f64>,
    vega: Option<f64>,
}

pub async fn fetch_deribit_option_summaries(
    client: &reqwest::Client,
    currency: &str,
) -> Result<Vec<OptionSummary>> {
    fetch_deribit_option_summaries_from(client, "https://www.deribit.com/api/v2/", currency).await
}

pub async fn fetch_deribit_option_summaries_from(
    client: &reqwest::Client,
    base_url: &str,
    currency: &str,
) -> Result<Vec<OptionSummary>> {
    let currency = currency.trim().to_ascii_uppercase();
    let mut url = Url::parse(base_url)?.join("public/get_book_summary_by_currency")?;
    url.query_pairs_mut()
        .append_pair("currency", &currency)
        .append_pair("kind", "option");
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<DeribitResponse<Vec<DeribitRawSummary>>>()
        .await
        .context("failed to decode deribit option summaries")?;

    Ok(response
        .result
        .into_iter()
        .map(|raw| {
            let parsed = parse_deribit_instrument(&raw.instrument_name);
            OptionSummary {
                venue: "deribit".to_string(),
                currency: currency.clone(),
                instrument_name: raw.instrument_name,
                option_type: parsed.as_ref().map(|p| p.2.clone()),
                strike: parsed.as_ref().map(|p| p.1),
                expiry_time: parsed.map(|p| p.0),
                bid_price: raw.bid_price,
                ask_price: raw.ask_price,
                mark_price: raw.mark_price,
                mark_iv: raw.mark_iv,
                delta: None,
                gamma: None,
                theta: None,
                vega: None,
                underlying_price: raw.underlying_price,
                underlying_index: raw.underlying_index,
                open_interest: raw.open_interest,
            }
        })
        .collect())
}

pub async fn fetch_deribit_option_book_from(
    client: &reqwest::Client,
    base_url: &str,
    instrument_name: &str,
    depth: usize,
) -> Result<DeribitOptionBook> {
    let mut url = Url::parse(base_url)?.join("public/get_order_book")?;
    url.query_pairs_mut()
        .append_pair("instrument_name", instrument_name)
        .append_pair("depth", &depth.clamp(1, 100).to_string());
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<DeribitRawBookResponse>()
        .await
        .context("failed to decode deribit option order book")?;
    Ok(response.result.into_book())
}

impl DeribitRawBook {
    fn into_book(self) -> DeribitOptionBook {
        DeribitOptionBook {
            venue: "deribit".to_string(),
            instrument_name: self.instrument_name,
            timestamp: self.timestamp,
            state: self.state,
            bid_price: self.best_bid_price,
            ask_price: self.best_ask_price,
            mark_price: self.mark_price,
            mark_iv: self.mark_iv,
            underlying_price: self.underlying_price,
            underlying_index: self.underlying_index,
            open_interest: self.open_interest,
            delta: self.greeks.as_ref().and_then(|g| g.delta),
            gamma: self.greeks.as_ref().and_then(|g| g.gamma),
            theta: self.greeks.as_ref().and_then(|g| g.theta),
            vega: self.greeks.as_ref().and_then(|g| g.vega),
            bids: deribit_levels(self.bids),
            asks: deribit_levels(self.asks),
        }
    }
}

fn deribit_levels(rows: Vec<Vec<f64>>) -> Vec<BookLevel> {
    rows.into_iter()
        .filter_map(|row| {
            Some(BookLevel {
                price: *row.first()?,
                qty: *row.get(1)?,
            })
        })
        .collect()
}

fn parse_deribit_instrument(name: &str) -> Option<(String, f64, String)> {
    let parts = name.split('-').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    let expiry_time = parse_deribit_date(parts[1])?;
    let strike = parts[2].parse::<f64>().ok()?;
    let option_type = option_side_from_code(parts[3]);
    Some((expiry_time, strike, option_type))
}

fn parse_deribit_date(text: &str) -> Option<String> {
    parse_day_month_year_expiry(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deribit_order_book_extracts_greeks_and_depth() {
        let raw = DeribitRawBook {
            instrument_name: "BTC-29MAY26-70000-P".to_string(),
            timestamp: Some(1),
            state: Some("open".to_string()),
            best_bid_price: Some(0.0016),
            best_ask_price: Some(0.0019),
            mark_price: Some(0.0017),
            mark_iv: Some(46.4),
            underlying_price: Some(77_967.19),
            underlying_index: Some("BTC-29MAY26".to_string()),
            open_interest: Some(3544.6),
            greeks: Some(DeribitGreeks {
                delta: Some(-0.05645),
                gamma: Some(0.00002),
                theta: Some(-37.55606),
                vega: Some(13.26425),
            }),
            bids: vec![vec![0.0016, 2.4]],
            asks: vec![vec![0.0019, 40.3]],
        };
        let book = raw.into_book();
        assert_eq!(book.delta, Some(-0.05645));
        assert_eq!(book.bids[0].price, 0.0016);
        assert_eq!(book.asks[0].qty, 40.3);
    }
}
