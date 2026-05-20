use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::common::{
    OptionSummary, option_side_from_code, parse_day_month_year_expiry, parse_f64_opt,
};

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
                underlying_price: parse_f64_opt(raw.underlying_price.as_deref())
                    .or_else(|| parse_f64_opt(raw.index_price.as_deref())),
                underlying_index: Some(currency.clone()),
                open_interest: parse_f64_opt(raw.open_interest.as_deref()),
            }
        })
        .collect())
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
}
