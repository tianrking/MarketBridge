use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct DeribitOptionSummary {
    pub currency: String,
    pub instrument_name: String,
    pub option_type: Option<String>,
    pub strike: Option<f64>,
    pub expiry_time: Option<String>,
    pub bid_price: Option<f64>,
    pub ask_price: Option<f64>,
    pub mark_price: Option<f64>,
    pub mark_iv: Option<f64>,
    pub underlying_price: Option<f64>,
    pub underlying_index: Option<String>,
    pub open_interest: Option<f64>,
}

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
) -> Result<Vec<DeribitOptionSummary>> {
    fetch_deribit_option_summaries_from(client, "https://www.deribit.com/api/v2/", currency).await
}

pub async fn fetch_deribit_option_summaries_from(
    client: &reqwest::Client,
    base_url: &str,
    currency: &str,
) -> Result<Vec<DeribitOptionSummary>> {
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
            DeribitOptionSummary {
                currency: currency.clone(),
                instrument_name: raw.instrument_name,
                option_type: parsed.as_ref().map(|p| p.2.clone()),
                strike: parsed.as_ref().map(|p| p.1),
                expiry_time: parsed.map(|p| p.0),
                bid_price: raw.bid_price,
                ask_price: raw.ask_price,
                mark_price: raw.mark_price,
                mark_iv: raw.mark_iv,
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
    let option_type = match parts[3] {
        "C" => "call",
        "P" => "put",
        other => other,
    }
    .to_string();
    Some((expiry_time, strike, option_type))
}

fn parse_deribit_date(text: &str) -> Option<String> {
    if text.len() < 7 {
        return None;
    }
    let day = text[0..2].parse::<u32>().ok()?;
    let month = match &text[2..5].to_uppercase()[..] {
        "JAN" => 1,
        "FEB" => 2,
        "MAR" => 3,
        "APR" => 4,
        "MAY" => 5,
        "JUN" => 6,
        "JUL" => 7,
        "AUG" => 8,
        "SEP" => 9,
        "OCT" => 10,
        "NOV" => 11,
        "DEC" => 12,
        _ => return None,
    };
    let year = 2000 + text[5..7].parse::<i32>().ok()?;
    Some(format!("{year:04}-{month:02}-{day:02}T08:00:00Z"))
}
