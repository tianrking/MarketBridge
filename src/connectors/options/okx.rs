use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::common::{OptionSummary, option_side_from_code, parse_f64_opt, parse_yy_mm_dd_expiry};

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
                delta: None,
                gamma: None,
                theta: None,
                vega: None,
                underlying_price: parse_f64_opt(raw.idx_px.as_deref())
                    .or_else(|| parse_f64_opt(raw.fwd_px.as_deref())),
                underlying_index: raw.uly,
                open_interest: None,
            }
        })
        .collect())
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
}
