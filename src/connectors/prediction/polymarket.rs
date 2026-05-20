use anyhow::{Context, Result, bail};
use futures_util::future::join_all;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

const CLOB_BASE_URL: &str = "https://clob.polymarket.com";
pub const POLYMARKET_BATCH_TOKEN_LIMIT: usize = 500;
pub const POLYMARKET_HISTORY_BATCH_LIMIT: usize = 20;

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

#[derive(Debug, Clone, Serialize)]
pub struct PolymarketBookRequest {
    pub token_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolymarketLastTradePrice {
    pub token_id: String,
    pub price: String,
    pub side: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolymarketPriceHistoryPoint {
    pub t: u64,
    pub p: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolymarketPriceHistoryResponse {
    #[serde(default)]
    pub history: Vec<PolymarketPriceHistoryPoint>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolymarketBatchPriceHistoryResponse {
    #[serde(default)]
    pub history: HashMap<String, Vec<PolymarketPriceHistoryPoint>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolymarketBatchPriceHistoryRequest {
    pub markets: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ts: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ts: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fidelity: Option<u32>,
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
    let mut url = Url::parse(CLOB_BASE_URL)?.join("book")?;
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
    join_all(
        token_ids
            .iter()
            .map(|token_id| fetch_polymarket_book(client, token_id)),
    )
    .await
}

pub async fn fetch_polymarket_midpoints(
    client: &reqwest::Client,
    token_ids: &[String],
) -> Result<HashMap<String, String>> {
    let requests = book_requests(token_ids, None)?;
    post_clob_json(client, "midpoints", &requests)
        .await
        .context("failed to fetch polymarket CLOB midpoints")
}

pub async fn fetch_polymarket_spreads(
    client: &reqwest::Client,
    token_ids: &[String],
) -> Result<HashMap<String, String>> {
    let requests = book_requests(token_ids, None)?;
    post_clob_json(client, "spreads", &requests)
        .await
        .context("failed to fetch polymarket CLOB spreads")
}

pub async fn fetch_polymarket_last_trade_prices(
    client: &reqwest::Client,
    token_ids: &[String],
) -> Result<Vec<PolymarketLastTradePrice>> {
    let requests = book_requests(token_ids, None)?;
    post_clob_json(client, "last-trades-prices", &requests)
        .await
        .context("failed to fetch polymarket CLOB last trade prices")
}

pub async fn fetch_polymarket_market_prices(
    client: &reqwest::Client,
    token_ids: &[String],
    sides: &[String],
) -> Result<Value> {
    let requests = price_requests(token_ids, sides)?;
    post_clob_json(client, "prices", &requests)
        .await
        .context("failed to fetch polymarket CLOB prices")
}

pub async fn fetch_polymarket_prices_history(
    client: &reqwest::Client,
    token_id: &str,
    start_ts: Option<f64>,
    end_ts: Option<f64>,
    interval: Option<&str>,
    fidelity: Option<u32>,
) -> Result<PolymarketPriceHistoryResponse> {
    let mut url = Url::parse(CLOB_BASE_URL)?.join("prices-history")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("market", token_id);
        if let Some(start_ts) = start_ts {
            pairs.append_pair("startTs", &start_ts.to_string());
        }
        if let Some(end_ts) = end_ts {
            pairs.append_pair("endTs", &end_ts.to_string());
        }
        if let Some(interval) = interval {
            pairs.append_pair("interval", interval);
        }
        if let Some(fidelity) = fidelity {
            pairs.append_pair("fidelity", &fidelity.to_string());
        }
    }
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<PolymarketPriceHistoryResponse>()
        .await
        .context("failed to decode polymarket CLOB price history")
}

pub async fn fetch_polymarket_batch_prices_history(
    client: &reqwest::Client,
    request: &PolymarketBatchPriceHistoryRequest,
) -> Result<PolymarketBatchPriceHistoryResponse> {
    if request.markets.is_empty() {
        bail!("at least one token id is required");
    }
    if request.markets.len() > POLYMARKET_HISTORY_BATCH_LIMIT {
        bail!(
            "polymarket batch price history supports at most {} token ids",
            POLYMARKET_HISTORY_BATCH_LIMIT
        );
    }
    post_clob_json(client, "batch-prices-history", request)
        .await
        .context("failed to fetch polymarket CLOB batch price history")
}

fn book_requests(token_ids: &[String], side: Option<&str>) -> Result<Vec<PolymarketBookRequest>> {
    validate_token_batch(token_ids, POLYMARKET_BATCH_TOKEN_LIMIT)?;
    Ok(token_ids
        .iter()
        .map(|token_id| PolymarketBookRequest {
            token_id: token_id.clone(),
            side: side.map(str::to_string),
        })
        .collect())
}

fn price_requests(token_ids: &[String], sides: &[String]) -> Result<Vec<PolymarketBookRequest>> {
    validate_token_batch(token_ids, POLYMARKET_BATCH_TOKEN_LIMIT)?;
    let sides = normalize_sides(sides)?;
    Ok(token_ids
        .iter()
        .flat_map(|token_id| {
            sides.iter().map(|side| PolymarketBookRequest {
                token_id: token_id.clone(),
                side: Some(side.clone()),
            })
        })
        .collect())
}

fn normalize_sides(sides: &[String]) -> Result<Vec<String>> {
    let mut out = if sides.is_empty() {
        vec!["BUY".to_string(), "SELL".to_string()]
    } else {
        sides
            .iter()
            .map(|side| side.trim().to_ascii_uppercase())
            .filter(|side| !side.is_empty())
            .collect::<Vec<_>>()
    };
    out.sort();
    out.dedup();
    if out.is_empty() || out.iter().any(|side| side != "BUY" && side != "SELL") {
        bail!("sides must be BUY, SELL, or both");
    }
    Ok(out)
}

fn validate_token_batch(token_ids: &[String], limit: usize) -> Result<()> {
    if token_ids.is_empty() {
        bail!("at least one token id is required");
    }
    if token_ids.len() > limit {
        bail!("polymarket batch endpoint supports at most {limit} token ids");
    }
    Ok(())
}

async fn post_clob_json<T: serde::de::DeserializeOwned, B: Serialize + ?Sized>(
    client: &reqwest::Client,
    path: &str,
    body: &B,
) -> Result<T> {
    let url = Url::parse(CLOB_BASE_URL)?.join(path)?;
    client
        .post(url)
        .json(body)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await
        .with_context(|| format!("failed to decode polymarket CLOB {path} response"))
}

pub fn summarize_book(book: PolymarketOrderBook) -> PolymarketBookSummary {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_requests_default_to_both_sides() {
        let token_ids = vec!["token-a".to_string()];
        let requests = price_requests(&token_ids, &[]).expect("price requests");

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].side.as_deref(), Some("BUY"));
        assert_eq!(requests[1].side.as_deref(), Some("SELL"));
    }

    #[test]
    fn price_requests_reject_invalid_side() {
        let token_ids = vec!["token-a".to_string()];
        let sides = vec!["MID".to_string()];

        assert!(price_requests(&token_ids, &sides).is_err());
    }

    #[test]
    fn book_requests_enforce_polymarket_batch_limit() {
        let token_ids = (0..=POLYMARKET_BATCH_TOKEN_LIMIT)
            .map(|i| format!("token-{i}"))
            .collect::<Vec<_>>();

        assert!(book_requests(&token_ids, None).is_err());
    }

    #[test]
    fn batch_history_request_uses_snake_case_fields() {
        let request = PolymarketBatchPriceHistoryRequest {
            markets: vec!["token-a".to_string()],
            start_ts: Some(1.0),
            end_ts: Some(2.0),
            interval: Some("1h".to_string()),
            fidelity: Some(1),
        };

        let value = serde_json::to_value(request).expect("json");

        assert_eq!(value["markets"][0], "token-a");
        assert_eq!(value["start_ts"], 1.0);
        assert_eq!(value["end_ts"], 2.0);
    }
}
