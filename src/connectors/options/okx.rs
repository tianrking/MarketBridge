use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::common::{
    OptionBook, OptionSummary, option_side_from_code, parse_f64_opt, parse_yy_mm_dd_expiry,
};
use crate::types::BookLevel;

#[derive(Debug, Deserialize)]
struct OkxResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OkxOptionSummary {
    inst_id: String,
    #[serde(default)]
    uly: Option<String>,
    #[serde(default)]
    opt_type: Option<String>,
    #[serde(default)]
    stk: Option<String>,
    #[serde(default)]
    bid_vol: Option<String>,
    #[serde(default)]
    ask_vol: Option<String>,
    #[serde(default)]
    mark_vol: Option<String>,
    #[serde(default)]
    fwd_px: Option<String>,
    #[serde(default)]
    idx_px: Option<String>,
    #[serde(default)]
    delta_bs: Option<String>,
    #[serde(default)]
    gamma_bs: Option<String>,
    #[serde(default)]
    theta_bs: Option<String>,
    #[serde(default)]
    vega_bs: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OkxOptionBookRow {
    #[serde(default)]
    bids: Vec<Vec<String>>,
    #[serde(default)]
    asks: Vec<Vec<String>>,
    #[serde(default)]
    ts: Option<String>,
}

pub async fn fetch_okx_option_summaries_from(
    client: &reqwest::Client,
    base_url: &str,
    currency: &str,
) -> Result<Vec<OptionSummary>> {
    let currency = currency.trim().to_ascii_uppercase();
    let uly = format!("{currency}-USD");
    let mut url = Url::parse(base_url)?.join("public/opt-summary")?;
    url.query_pairs_mut().append_pair("uly", &uly);

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<OkxResponse<Vec<OkxOptionSummary>>>()
        .await
        .context("failed to decode okx option summaries")?;

    Ok(response
        .data
        .into_iter()
        .map(|raw| {
            let parsed = parse_okx_instrument(&raw.inst_id);
            OptionSummary {
                venue: "okx".to_string(),
                currency: currency.clone(),
                instrument_name: raw.inst_id,
                option_type: raw
                    .opt_type
                    .as_deref()
                    .map(option_side_from_code)
                    .or_else(|| parsed.as_ref().map(|p| p.2.clone())),
                strike: parse_f64_opt(raw.stk.as_deref()).or_else(|| parsed.as_ref().map(|p| p.1)),
                expiry_time: parsed.map(|p| p.0),
                bid_price: None,
                ask_price: None,
                mark_price: None,
                mark_iv: parse_f64_opt(raw.mark_vol.as_deref())
                    .or_else(|| parse_f64_opt(raw.bid_vol.as_deref()))
                    .or_else(|| parse_f64_opt(raw.ask_vol.as_deref())),
                delta: parse_f64_opt(raw.delta_bs.as_deref()),
                gamma: parse_f64_opt(raw.gamma_bs.as_deref()),
                theta: parse_f64_opt(raw.theta_bs.as_deref()),
                vega: parse_f64_opt(raw.vega_bs.as_deref()),
                underlying_price: parse_f64_opt(raw.idx_px.as_deref())
                    .or_else(|| parse_f64_opt(raw.fwd_px.as_deref())),
                underlying_index: raw.uly,
                open_interest: None,
            }
        })
        .collect())
}

pub async fn fetch_okx_option_book_from(
    client: &reqwest::Client,
    base_url: &str,
    instrument_name: &str,
    depth: usize,
) -> Result<OptionBook> {
    let mut url = Url::parse(base_url)?.join("market/books")?;
    url.query_pairs_mut()
        .append_pair("instId", instrument_name)
        .append_pair("sz", &depth.clamp(1, 400).to_string());

    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<OkxResponse<Vec<OkxOptionBookRow>>>()
        .await
        .context("failed to decode okx option order book")?;

    let row = response
        .data
        .into_iter()
        .next()
        .context("okx option order book response was empty")?;
    Ok(row.into_book(instrument_name))
}

impl OkxOptionBookRow {
    fn into_book(self, instrument_name: &str) -> OptionBook {
        let bids = okx_levels(self.bids);
        let asks = okx_levels(self.asks);
        OptionBook {
            venue: "okx".to_string(),
            instrument_name: instrument_name.to_string(),
            timestamp: self.ts.and_then(|value| value.parse::<u64>().ok()),
            state: None,
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

fn okx_levels(rows: Vec<Vec<String>>) -> Vec<BookLevel> {
    rows.into_iter()
        .filter_map(|row| {
            Some(BookLevel {
                price: parse_f64_opt(row.first().map(String::as_str))?,
                qty: parse_f64_opt(row.get(1).map(String::as_str))?,
            })
        })
        .collect()
}

fn parse_okx_instrument(name: &str) -> Option<(String, f64, String)> {
    let parts = name.split('-').collect::<Vec<_>>();
    if parts.len() < 5 {
        return None;
    }
    let expiry_time = parse_yy_mm_dd_expiry(parts[2])?;
    let strike = parts[3].parse::<f64>().ok()?;
    let option_type = option_side_from_code(parts[4]);
    Some((expiry_time, strike, option_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_okx_option_symbol() {
        let parsed = parse_okx_instrument("BTC-USD-260626-100000-C").unwrap();
        assert_eq!(parsed.0, "2026-06-26T08:00:00Z");
        assert_eq!(parsed.1, 100_000.0);
        assert_eq!(parsed.2, "call");
    }

    #[test]
    fn okx_summary_carries_black_scholes_greeks() {
        let raw = OkxOptionSummary {
            inst_id: "BTC-USD-260626-100000-C".to_string(),
            uly: Some("BTC-USD".to_string()),
            opt_type: Some("C".to_string()),
            stk: Some("100000".to_string()),
            bid_vol: None,
            ask_vol: None,
            mark_vol: Some("0.45".to_string()),
            fwd_px: None,
            idx_px: Some("78000".to_string()),
            delta_bs: Some("0.25".to_string()),
            gamma_bs: Some("0.00001".to_string()),
            theta_bs: Some("-12.5".to_string()),
            vega_bs: Some("50.5".to_string()),
        };
        assert_eq!(parse_f64_opt(raw.delta_bs.as_deref()), Some(0.25));
        assert_eq!(parse_f64_opt(raw.gamma_bs.as_deref()), Some(0.00001));
        assert_eq!(parse_f64_opt(raw.theta_bs.as_deref()), Some(-12.5));
        assert_eq!(parse_f64_opt(raw.vega_bs.as_deref()), Some(50.5));
    }

    #[test]
    fn okx_option_book_extracts_depth() {
        let book = OkxOptionBookRow {
            bids: vec![vec![
                "0.021".to_string(),
                "12".to_string(),
                "0".to_string(),
                "1".to_string(),
            ]],
            asks: vec![vec![
                "0.023".to_string(),
                "10".to_string(),
                "0".to_string(),
                "1".to_string(),
            ]],
            ts: Some("1779334083503".to_string()),
        }
        .into_book("BTC-USD-260626-100000-C");
        assert_eq!(book.bid_price, Some(0.021));
        assert_eq!(book.ask_price, Some(0.023));
        assert_eq!(book.timestamp, Some(1779334083503));
    }
}
