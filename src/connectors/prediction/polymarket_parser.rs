use serde::Deserialize;
use serde_json::Value;

use super::polymarket::PolymarketCryptoMarket;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaMarket {
    pub condition_id: Option<String>,
    pub slug: Option<String>,
    pub event_slug: Option<String>,
    pub category: Option<String>,
    pub question: Option<String>,
    pub end_date: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub clob_token_ids: Option<Value>,
    pub outcomes: Option<Value>,
}

pub fn parse_crypto_market(market: &GammaMarket) -> Option<PolymarketCryptoMarket> {
    let status = if market.closed == Some(true) {
        "closed"
    } else if market.active == Some(true) {
        "active"
    } else {
        "unknown"
    };
    if status != "active" {
        return None;
    }

    let question = market.question.clone().unwrap_or_default();
    let slug = market.slug.clone().unwrap_or_default();
    let haystack = format!(
        "{} {} {} {}",
        question,
        slug,
        market.event_slug.clone().unwrap_or_default(),
        market.category.clone().unwrap_or_default()
    )
    .to_lowercase();
    let base_asset = parse_base_asset(&haystack)?;
    let strike = extract_strike(&haystack, &base_asset)?;
    let direction = parse_direction(&haystack).unwrap_or_else(|| "above".to_string());
    let rule_type = if haystack.contains("touch")
        || haystack.contains("hit ")
        || haystack.contains("reach")
        || haystack.contains("at any point")
    {
        "touch".to_string()
    } else {
        "terminal".to_string()
    };

    let tokens = string_vec(market.clob_token_ids.as_ref());
    let outcomes = string_vec(market.outcomes.as_ref());
    let mut yes_token_id = None;
    let mut no_token_id = None;
    for (index, outcome) in outcomes.iter().enumerate() {
        let label = outcome.to_lowercase();
        if label == "yes" || label.contains("yes") {
            yes_token_id = tokens.get(index).cloned();
        }
        if label == "no" || label.contains("no") {
            no_token_id = tokens.get(index).cloned();
        }
    }
    if yes_token_id.is_none() && tokens.len() >= 2 {
        yes_token_id = tokens.first().cloned();
        no_token_id = tokens.get(1).cloned();
    }

    let market_id = market
        .condition_id
        .clone()
        .or_else(|| market.slug.clone())
        .or_else(|| market.event_slug.clone())?;
    let mut parser_confidence: f64 = 0.50;
    if yes_token_id.is_some() && no_token_id.is_some() {
        parser_confidence += 0.20;
    }
    if market.end_date.is_some() {
        parser_confidence += 0.15;
    }
    if rule_type == "terminal" {
        parser_confidence += 0.15;
    }

    Some(PolymarketCryptoMarket {
        market_id,
        condition_id: market.condition_id.clone(),
        market_slug: market.slug.clone(),
        event_slug: market.event_slug.clone(),
        base_asset,
        quote_asset: "USD".to_string(),
        strike,
        direction,
        rule_type,
        expiry_time: market.end_date.clone(),
        yes_token_id,
        no_token_id,
        question: market.question.clone(),
        status: status.to_string(),
        parser_confidence: parser_confidence.min(1.0),
    })
}

fn parse_base_asset(text: &str) -> Option<String> {
    let words = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words
        .iter()
        .any(|word| *word == "bitcoin" || *word == "btc")
    {
        Some("BTC".to_string())
    } else if words
        .iter()
        .any(|word| *word == "ethereum" || *word == "ether" || *word == "eth")
    {
        Some("ETH".to_string())
    } else {
        None
    }
}

fn parse_direction(text: &str) -> Option<String> {
    if text.contains("below")
        || text.contains("under")
        || text.contains("lower than")
        || text.contains("less than")
    {
        Some("below".to_string())
    } else if text.contains("above")
        || text.contains("over")
        || text.contains("higher than")
        || text.contains("greater than")
    {
        Some("above".to_string())
    } else {
        None
    }
}

fn extract_strike(text: &str, base_asset: &str) -> Option<f64> {
    for (index, ch) in text.char_indices() {
        if ch != '$' {
            continue;
        }
        let raw = text[index + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',' || *c == 'k' || *c == 'm')
            .collect::<String>();
        if let Some(value) = parse_amount_token(&raw) {
            return Some(value);
        }
    }

    let cleaned = text
        .replace(['$', ',', '?', '(', ')', ':', ';'], " ")
        .replace('-', " ");
    for token in cleaned.split_whitespace() {
        let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
        let value = parse_amount_token(token)?;
        let min_strike = if base_asset == "BTC" { 5_000.0 } else { 100.0 };
        if (min_strike..=2_000_000.0).contains(&value) && !(1900.0..=2100.0).contains(&value) {
            return Some(value);
        }
    }
    None
}

fn parse_amount_token(token: &str) -> Option<f64> {
    let token = token.trim().replace(',', "");
    if token.is_empty() {
        return None;
    }
    if let Some(raw) = token.strip_suffix('k') {
        raw.parse::<f64>().ok().map(|x| x * 1_000.0)
    } else if let Some(raw) = token.strip_suffix('m') {
        raw.parse::<f64>().ok().map(|x| x * 1_000_000.0)
    } else {
        token.parse::<f64>().ok()
    }
}

fn string_vec(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect(),
        Some(Value::String(text)) => serde_json::from_str::<Vec<String>>(text).unwrap_or_default(),
        _ => Vec::new(),
    }
}
