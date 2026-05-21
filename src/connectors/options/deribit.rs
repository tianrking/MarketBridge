use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::common::{OptionSummary, option_side_from_code, parse_day_month_year_expiry};

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
