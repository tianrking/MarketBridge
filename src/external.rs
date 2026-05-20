use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Clone, Serialize)]
pub struct PolymarketCryptoMarket {
    pub market_id: String,
    pub condition_id: Option<String>,
    pub market_slug: Option<String>,
    pub event_slug: Option<String>,
    pub base_asset: String,
    pub quote_asset: String,
    pub strike: f64,
    pub direction: String,
    pub rule_type: String,
    pub expiry_time: Option<String>,
    pub yes_token_id: Option<String>,
    pub no_token_id: Option<String>,
    pub question: Option<String>,
    pub status: String,
    pub parser_confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolymarketCryptoMarketsResponse {
    pub markets: Vec<PolymarketCryptoMarket>,
    pub clob_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolymarketBookLevel {
    pub price: String,
    pub size: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolymarketOrderBook {
    pub market: Option<String>,
    pub asset_id: String,
    pub timestamp: Option<String>,
    pub hash: Option<String>,
    #[serde(default)]
    pub bids: Vec<PolymarketBookLevel>,
    #[serde(default)]
    pub asks: Vec<PolymarketBookLevel>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolymarketBookSummary {
    pub market: Option<String>,
    pub asset_id: String,
    pub timestamp: Option<String>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub raw_bid_levels: usize,
    pub raw_ask_levels: usize,
    pub book: PolymarketOrderBook,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GammaMarket {
    condition_id: Option<String>,
    slug: Option<String>,
    event_slug: Option<String>,
    category: Option<String>,
    question: Option<String>,
    end_date: Option<String>,
    active: Option<bool>,
    closed: Option<bool>,
    clob_token_ids: Option<Value>,
    outcomes: Option<Value>,
}

pub async fn fetch_deribit_option_summaries(
    client: &reqwest::Client,
    currency: &str,
) -> Result<Vec<DeribitOptionSummary>> {
    let currency = currency.trim().to_ascii_uppercase();
    let url = format!(
        "https://www.deribit.com/api/v2/public/get_book_summary_by_currency?currency={currency}&kind=option"
    );
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

pub async fn fetch_polymarket_crypto_markets(
    client: &reqwest::Client,
    gamma_base_url: &str,
    limit: usize,
    max_offset: usize,
) -> Result<PolymarketCryptoMarketsResponse> {
    let mut markets = Vec::new();
    let mut offset = 0usize;
    while offset <= max_offset {
        let batch = fetch_gamma_markets(client, gamma_base_url, limit, offset).await?;
        if batch.is_empty() {
            break;
        }
        markets.extend(batch.iter().filter_map(parse_crypto_market));
        offset += limit;
    }

    let mut clob_asset_ids = markets
        .iter()
        .flat_map(|market| [market.yes_token_id.as_ref(), market.no_token_id.as_ref()])
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    clob_asset_ids.sort();
    clob_asset_ids.dedup();

    Ok(PolymarketCryptoMarketsResponse {
        markets,
        clob_asset_ids,
    })
}

pub async fn fetch_polymarket_book(
    client: &reqwest::Client,
    token_id: &str,
) -> Result<PolymarketBookSummary> {
    let mut url = Url::parse("https://clob.polymarket.com/book")?;
    url.query_pairs_mut().append_pair("token_id", token_id);
    let book = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<PolymarketOrderBook>()
        .await
        .context("failed to decode polymarket CLOB book")?;
    Ok(summarize_book(book))
}

pub async fn fetch_polymarket_books(
    client: &reqwest::Client,
    token_ids: &[String],
) -> Vec<Result<PolymarketBookSummary>> {
    let mut out = Vec::with_capacity(token_ids.len());
    for token_id in token_ids {
        out.push(fetch_polymarket_book(client, token_id).await);
    }
    out
}

fn summarize_book(book: PolymarketOrderBook) -> PolymarketBookSummary {
    let best_bid = book
        .bids
        .iter()
        .filter_map(|level| level.price.parse::<f64>().ok())
        .reduce(f64::max);
    let best_ask = book
        .asks
        .iter()
        .filter_map(|level| level.price.parse::<f64>().ok())
        .reduce(f64::min);
    let bid_depth = book
        .bids
        .iter()
        .filter_map(|level| level.size.parse::<f64>().ok())
        .sum();
    let ask_depth = book
        .asks
        .iter()
        .filter_map(|level| level.size.parse::<f64>().ok())
        .sum();
    PolymarketBookSummary {
        market: book.market.clone(),
        asset_id: book.asset_id.clone(),
        timestamp: book.timestamp.clone(),
        best_bid,
        best_ask,
        spread: best_bid.zip(best_ask).map(|(bid, ask)| ask - bid),
        bid_depth,
        ask_depth,
        raw_bid_levels: book.bids.len(),
        raw_ask_levels: book.asks.len(),
        book,
    }
}

async fn fetch_gamma_markets(
    client: &reqwest::Client,
    base_url: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<GammaMarket>> {
    let mut url = Url::parse(base_url)?.join("markets")?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair("offset", &offset.to_string())
        .append_pair("active", "true")
        .append_pair("closed", "false");
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<GammaMarket>>()
        .await
        .context("failed to decode gamma markets")
}

fn parse_crypto_market(market: &GammaMarket) -> Option<PolymarketCryptoMarket> {
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
    if words.iter().any(|word| *word == "bitcoin" || *word == "btc") {
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
            .take_while(|c| {
                c.is_ascii_digit() || *c == '.' || *c == ',' || *c == 'k' || *c == 'm'
            })
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
