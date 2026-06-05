use anyhow::{Context, Result, anyhow};
use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;
use tokio::time::timeout;

const SUPPORTED_MARKET_EXCHANGES: &[&str] = &[
    "binance", "okx", "bybit", "bitget", "kucoin", "gate", "mexc", "bingx", "bitmart",
];

const SUPPORTED_PERPETUAL_FUNDING_EXCHANGES: &[&str] = &[
    "binance", "okx", "bybit", "bitget", "kucoin", "gate", "mexc", "bingx", "bitmart",
];
const DISCOVERY_HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const DISCOVERY_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(8);
const DISCOVERY_MAX_CONCURRENCY: usize = 6;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MarketDiscoveryQuery {
    pub exchange: Option<String>,
    pub exchanges: Option<String>,
    pub market: Option<String>,
    pub quote: Option<String>,
    pub base: Option<String>,
    pub active_only: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PerpetualDiscoveryQuery {
    pub exchange: Option<String>,
    pub exchanges: Option<String>,
    pub quote: Option<String>,
    pub base: Option<String>,
    pub active_only: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PerpetualFundingQuery {
    pub exchange: Option<String>,
    pub exchanges: Option<String>,
    pub symbols: Option<String>,
    pub quote: Option<String>,
    pub active_only: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketListing {
    pub exchange: String,
    pub market: String,
    pub symbol: String,
    pub native_symbol: String,
    pub base: Option<String>,
    pub quote: Option<String>,
    pub active: bool,
    pub status: Option<String>,
    pub contract_type: Option<String>,
    pub settle_asset: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerpetualExchangeGroup {
    pub exchange: String,
    pub contracts_total: usize,
    pub contracts_returned: usize,
    pub base_assets_total: usize,
    pub base_assets: Vec<String>,
    pub contracts: Vec<MarketListing>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerpetualFundingRow {
    pub exchange: String,
    pub symbol: String,
    pub native_symbol: String,
    pub funding_rate: f64,
    pub funding_rate_pct: f64,
    pub next_funding_time_ms: Option<u64>,
    pub mark_price: Option<f64>,
    pub index_price: Option<f64>,
    pub active: Option<bool>,
    pub source: String,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryError {
    pub exchange: String,
    pub error: String,
}

pub fn supported_market_exchanges() -> &'static [&'static str] {
    SUPPORTED_MARKET_EXCHANGES
}

pub fn supported_perpetual_funding_exchanges() -> &'static [&'static str] {
    SUPPORTED_PERPETUAL_FUNDING_EXCHANGES
}

pub async fn discover_markets(
    http: &reqwest::Client,
    query: &MarketDiscoveryQuery,
) -> (Vec<MarketListing>, Vec<DiscoveryError>) {
    let exchanges = requested_exchanges(
        query.exchange.as_deref(),
        query.exchanges.as_deref(),
        SUPPORTED_MARKET_EXCHANGES,
    );
    let market_filter = query.market.as_deref().map(normalize_lower);
    let active_only = query.active_only.unwrap_or(true);
    let mut rows = Vec::new();
    let mut errors = Vec::new();

    let results = stream::iter(exchanges.into_iter().map(|exchange| {
        let market_filter = market_filter.clone();
        async move {
            let result = timeout(
                DISCOVERY_EXCHANGE_TIMEOUT,
                discover_exchange_markets(http, &exchange, market_filter.as_deref()),
            )
            .await;
            match result {
                Ok(Ok(exchange_rows)) => Ok(exchange_rows),
                Ok(Err(error)) => Err(DiscoveryError {
                    exchange,
                    error: error.to_string(),
                }),
                Err(_) => Err(DiscoveryError {
                    exchange,
                    error: format!(
                        "market discovery timed out after {}ms",
                        DISCOVERY_EXCHANGE_TIMEOUT.as_millis()
                    ),
                }),
            }
        }
    }))
    .buffer_unordered(DISCOVERY_MAX_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    for result in results {
        match result {
            Ok(exchange_rows) => rows.extend(exchange_rows),
            Err(error) => errors.push(error),
        }
    }

    rows.retain(|row| {
        (!active_only || row.active)
            && query.quote.as_deref().is_none_or(|quote| {
                row.quote
                    .as_deref()
                    .is_some_and(|x| x.eq_ignore_ascii_case(quote))
            })
            && query.base.as_deref().is_none_or(|base| {
                row.base
                    .as_deref()
                    .is_some_and(|x| x.eq_ignore_ascii_case(base))
            })
    });
    rows.sort_by(|a, b| {
        a.exchange
            .cmp(&b.exchange)
            .then(a.market.cmp(&b.market))
            .then(a.symbol.cmp(&b.symbol))
    });
    rows.truncate(query.limit.unwrap_or(5000).clamp(1, 50_000));
    (rows, errors)
}

pub async fn discover_perpetuals(
    http: &reqwest::Client,
    query: &PerpetualDiscoveryQuery,
) -> (Vec<PerpetualExchangeGroup>, Vec<DiscoveryError>) {
    let market_query = MarketDiscoveryQuery {
        exchange: query.exchange.clone(),
        exchanges: query.exchanges.clone(),
        market: Some("perp".to_string()),
        quote: query.quote.clone(),
        base: query.base.clone(),
        active_only: query.active_only,
        limit: Some(50_000),
    };
    let (markets, errors) = discover_markets(http, &market_query).await;
    let per_exchange_limit = query.limit.unwrap_or(50_000).clamp(1, 50_000);
    let mut grouped = BTreeMap::<String, Vec<MarketListing>>::new();
    for market in markets {
        grouped
            .entry(market.exchange.clone())
            .or_default()
            .push(market);
    }

    let groups = grouped
        .into_iter()
        .map(|(exchange, mut contracts)| {
            contracts.sort_by(|a, b| a.symbol.cmp(&b.symbol));
            let contracts_total = contracts.len();
            let mut base_assets = contracts
                .iter()
                .filter_map(|contract| contract.base.clone())
                .collect::<Vec<_>>();
            base_assets.sort();
            base_assets.dedup();
            contracts.truncate(per_exchange_limit);
            PerpetualExchangeGroup {
                exchange,
                contracts_total,
                contracts_returned: contracts.len(),
                base_assets_total: base_assets.len(),
                base_assets,
                contracts,
            }
        })
        .collect();
    (groups, errors)
}

pub async fn fetch_perpetual_funding(
    http: &reqwest::Client,
    query: &PerpetualFundingQuery,
) -> (Vec<PerpetualFundingRow>, Vec<DiscoveryError>) {
    let exchanges = requested_exchanges(
        query.exchange.as_deref(),
        query.exchanges.as_deref(),
        SUPPORTED_PERPETUAL_FUNDING_EXCHANGES,
    );
    let active_only = query.active_only.unwrap_or(true);
    let symbols = query.symbols.as_deref().map(csv_upper_set);
    let mut rows = Vec::new();
    let mut errors = Vec::new();

    let results = stream::iter(exchanges.into_iter().map(|exchange| async move {
        let result = timeout(
            DISCOVERY_EXCHANGE_TIMEOUT,
            fetch_exchange_perpetual_funding(http, &exchange),
        )
        .await;
        match result {
            Ok(Ok(exchange_rows)) => Ok(exchange_rows),
            Ok(Err(error)) => Err(DiscoveryError {
                exchange,
                error: error.to_string(),
            }),
            Err(_) => Err(DiscoveryError {
                exchange,
                error: format!(
                    "perpetual funding discovery timed out after {}ms",
                    DISCOVERY_EXCHANGE_TIMEOUT.as_millis()
                ),
            }),
        }
    }))
    .buffer_unordered(DISCOVERY_MAX_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    for result in results {
        match result {
            Ok(exchange_rows) => rows.extend(exchange_rows),
            Err(error) => errors.push(error),
        }
    }

    rows.retain(|row| {
        (!active_only || row.active.unwrap_or(true))
            && query.quote.as_deref().is_none_or(|quote| {
                split_symbol(&row.symbol)
                    .1
                    .is_some_and(|row_quote| row_quote.eq_ignore_ascii_case(quote))
            })
            && symbols
                .as_ref()
                .is_none_or(|set| set.contains(&row.symbol.to_ascii_uppercase()))
    });
    rows.sort_by(|a, b| a.exchange.cmp(&b.exchange).then(a.symbol.cmp(&b.symbol)));
    rows.truncate(query.limit.unwrap_or(5000).clamp(1, 50_000));
    (rows, errors)
}

async fn discover_exchange_markets(
    http: &reqwest::Client,
    exchange: &str,
    market: Option<&str>,
) -> Result<Vec<MarketListing>> {
    let mut rows = Vec::new();
    if market.is_none_or(|m| m == "spot") {
        rows.extend(match exchange {
            "binance" => binance_markets(http, "spot").await?,
            "okx" => okx_markets(http, "spot").await?,
            "bybit" => bybit_markets(http, "spot").await?,
            "bitget" => bitget_spot_markets(http).await?,
            "kucoin" => kucoin_spot_markets(http).await?,
            "gate" => gate_spot_markets(http).await?,
            "mexc" => mexc_spot_markets(http).await?,
            "bitmart" => bitmart_spot_markets(http).await?,
            "bingx" => Vec::new(),
            other => return Err(anyhow!("unsupported market discovery exchange: {other}")),
        });
    }
    if market.is_none_or(|m| m == "perp" || m == "swap") {
        rows.extend(match exchange {
            "binance" => binance_markets(http, "perp").await?,
            "okx" => okx_markets(http, "perp").await?,
            "bybit" => bybit_markets(http, "perp").await?,
            "bitget" => bitget_perp_markets(http).await?,
            "kucoin" => kucoin_perp_contracts(http).await?.0,
            "gate" => gate_perp_contracts(http).await?.0,
            "mexc" => mexc_perp_contracts(http).await?.0,
            "bingx" => bingx_perp_contracts(http).await?.0,
            "bitmart" => bitmart_perp_details(http).await?.0,
            other => return Err(anyhow!("unsupported market discovery exchange: {other}")),
        });
    }
    Ok(rows)
}

async fn fetch_exchange_perpetual_funding(
    http: &reqwest::Client,
    exchange: &str,
) -> Result<Vec<PerpetualFundingRow>> {
    match exchange {
        "binance" => binance_funding(http).await,
        "okx" => okx_funding(http).await,
        "bybit" => bybit_funding(http).await,
        "bitget" => bitget_funding(http).await,
        "kucoin" => Ok(kucoin_perp_contracts(http).await?.1),
        "gate" => Ok(gate_perp_contracts(http).await?.1),
        "mexc" => Ok(mexc_perp_contracts(http).await?.1),
        "bingx" => Ok(bingx_perp_contracts(http).await?.1),
        "bitmart" => Ok(bitmart_perp_details(http).await?.1),
        other => Err(anyhow!("unsupported perpetual funding exchange: {other}")),
    }
}

async fn get_json(http: &reqwest::Client, url: &str) -> Result<Value> {
    http.get(url)
        .timeout(DISCOVERY_HTTP_TIMEOUT)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("non-success status: {url}"))?
        .json::<Value>()
        .await
        .with_context(|| format!("invalid JSON: {url}"))
}

async fn binance_markets(http: &reqwest::Client, market: &str) -> Result<Vec<MarketListing>> {
    let url = if market == "spot" {
        "https://api.binance.com/api/v3/exchangeInfo"
    } else {
        "https://fapi.binance.com/fapi/v1/exchangeInfo"
    };
    let value = get_json(http, url).await?;
    let rows = value
        .get("symbols")
        .and_then(Value::as_array)
        .context("binance symbols missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            if market == "perp" && text(row, "contractType") != Some("PERPETUAL") {
                return None;
            }
            let status = text(row, "status").map(str::to_string);
            let active = status.as_deref() == Some("TRADING");
            let base = text(row, "baseAsset").map(binance_base);
            let quote = text(row, "quoteAsset").map(str::to_ascii_uppercase);
            Some(market_listing(
                "binance",
                market,
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                status,
                text(row, "contractType").map(str::to_string),
                text(row, "marginAsset").map(str::to_string),
                url,
            ))
        })
        .collect();
    Ok(rows)
}

async fn binance_funding(http: &reqwest::Client) -> Result<Vec<PerpetualFundingRow>> {
    let url = "https://fapi.binance.com/fapi/v1/premiumIndex";
    let value = get_json(http, url).await?;
    Ok(value
        .as_array()
        .context("binance premiumIndex array missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let funding_rate = number(row, "lastFundingRate")?;
            Some(funding_row(
                "binance",
                native,
                native,
                funding_rate,
                time_ms(row, "nextFundingTime"),
                number(row, "markPrice"),
                number(row, "indexPrice"),
                Some(true),
                url,
                u64_value(row, "time"),
            ))
        })
        .collect())
}

async fn okx_markets(http: &reqwest::Client, market: &str) -> Result<Vec<MarketListing>> {
    let inst_type = if market == "spot" { "SPOT" } else { "SWAP" };
    let url = format!("https://www.okx.com/api/v5/public/instruments?instType={inst_type}");
    let value = get_json(http, &url).await?;
    Ok(value
        .get("data")
        .and_then(Value::as_array)
        .context("okx data missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "instId")?;
            if market == "perp" && !native.ends_with("-SWAP") {
                return None;
            }
            let (base, quote) = okx_base_quote(row, native);
            let status = text(row, "state").map(str::to_string);
            let active = status.as_deref() == Some("live");
            Some(market_listing(
                "okx",
                market,
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                status,
                text(row, "ctType").map(str::to_string),
                text(row, "settleCcy").map(str::to_string),
                &url,
            ))
        })
        .collect())
}

async fn okx_funding(http: &reqwest::Client) -> Result<Vec<PerpetualFundingRow>> {
    let markets = okx_markets(http, "perp").await?;
    let rows = stream::iter(markets.into_iter().filter(|m| m.active))
        .map(|market| async move {
            let url = format!(
                "https://www.okx.com/api/v5/public/funding-rate?instId={}",
                market.native_symbol
            );
            let value = get_json(http, &url).await?;
            let row = value
                .get("data")
                .and_then(Value::as_array)
                .and_then(|rows| rows.first())
                .context("okx funding data missing")?;
            let funding_rate = number(row, "fundingRate").context("okx fundingRate missing")?;
            Ok::<_, anyhow::Error>(funding_row(
                "okx",
                &market.symbol,
                &market.native_symbol,
                funding_rate,
                time_ms(row, "fundingTime").or_else(|| time_ms(row, "nextFundingTime")),
                None,
                None,
                Some(market.active),
                &url,
                u64_value(row, "ts"),
            ))
        })
        .buffer_unordered(12)
        .filter_map(|result| async move { result.ok() })
        .collect::<Vec<_>>()
        .await;
    Ok(rows)
}

async fn bybit_markets(http: &reqwest::Client, market: &str) -> Result<Vec<MarketListing>> {
    let category = if market == "spot" { "spot" } else { "linear" };
    let mut cursor = String::new();
    let mut rows = Vec::new();
    loop {
        let url = if cursor.is_empty() {
            format!(
                "https://api.bybit.com/v5/market/instruments-info?category={category}&limit=1000"
            )
        } else {
            format!(
                "https://api.bybit.com/v5/market/instruments-info?category={category}&limit=1000&cursor={cursor}"
            )
        };
        let value = get_json(http, &url).await?;
        let result = value.get("result").context("bybit result missing")?;
        for row in result
            .get("list")
            .and_then(Value::as_array)
            .context("bybit instrument list missing")?
        {
            let native = text(row, "symbol").unwrap_or_default();
            if native.is_empty() {
                continue;
            }
            if market == "perp" && text(row, "contractType") != Some("LinearPerpetual") {
                continue;
            }
            let status = text(row, "status").map(str::to_string);
            let active = status.as_deref() == Some("Trading");
            let base = text(row, "baseCoin").map(str::to_ascii_uppercase);
            let quote = text(row, "quoteCoin").map(str::to_ascii_uppercase);
            rows.push(market_listing(
                "bybit",
                market,
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                status,
                text(row, "contractType").map(str::to_string),
                None,
                &url,
            ));
        }
        cursor = result
            .get("nextPageCursor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if cursor.is_empty() {
            break;
        }
    }
    Ok(rows)
}

async fn bybit_funding(http: &reqwest::Client) -> Result<Vec<PerpetualFundingRow>> {
    let url = "https://api.bybit.com/v5/market/tickers?category=linear";
    let value = get_json(http, url).await?;
    Ok(value
        .pointer("/result/list")
        .and_then(Value::as_array)
        .context("bybit ticker list missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let funding_rate = number(row, "fundingRate")?;
            Some(funding_row(
                "bybit",
                native,
                native,
                funding_rate,
                time_ms(row, "nextFundingTime"),
                number(row, "markPrice"),
                number(row, "indexPrice"),
                Some(true),
                url,
                None,
            ))
        })
        .collect())
}

async fn bitget_spot_markets(http: &reqwest::Client) -> Result<Vec<MarketListing>> {
    let url = "https://api.bitget.com/api/v2/spot/public/symbols";
    let value = get_json(http, url).await?;
    Ok(value
        .get("data")
        .and_then(Value::as_array)
        .context("bitget spot symbols missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let base = text(row, "baseCoin").map(str::to_ascii_uppercase);
            let quote = text(row, "quoteCoin").map(str::to_ascii_uppercase);
            let status = text(row, "status").map(str::to_string);
            let active = status
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("online"));
            Some(market_listing(
                "bitget",
                "spot",
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                status,
                None,
                None,
                url,
            ))
        })
        .collect())
}

async fn bitget_perp_markets(http: &reqwest::Client) -> Result<Vec<MarketListing>> {
    let url = "https://api.bitget.com/api/v2/mix/market/contracts?productType=USDT-FUTURES";
    let value = get_json(http, url).await?;
    Ok(value
        .get("data")
        .and_then(Value::as_array)
        .context("bitget perp contracts missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let base = text(row, "baseCoin").map(str::to_ascii_uppercase);
            let quote = text(row, "quoteCoin").map(str::to_ascii_uppercase);
            let status = text(row, "symbolStatus").map(str::to_string);
            let active = status
                .as_deref()
                .is_none_or(|s| s.eq_ignore_ascii_case("normal"));
            Some(market_listing(
                "bitget",
                "perp",
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                status,
                text(row, "symbolType").map(str::to_string),
                text(row, "quoteCoin").map(str::to_string),
                url,
            ))
        })
        .collect())
}

async fn bitget_funding(http: &reqwest::Client) -> Result<Vec<PerpetualFundingRow>> {
    let url = "https://api.bitget.com/api/v2/mix/market/tickers?productType=USDT-FUTURES";
    let value = get_json(http, url).await?;
    Ok(value
        .get("data")
        .and_then(Value::as_array)
        .context("bitget tickers missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let funding_rate = number(row, "fundingRate")?;
            Some(funding_row(
                "bitget",
                native,
                native,
                funding_rate,
                time_ms(row, "nextFundingTime"),
                number(row, "markPrice"),
                number(row, "indexPrice"),
                Some(true),
                url,
                u64_value(row, "ts"),
            ))
        })
        .collect())
}

async fn kucoin_spot_markets(http: &reqwest::Client) -> Result<Vec<MarketListing>> {
    let url = "https://api.kucoin.com/api/v2/symbols";
    let value = get_json(http, url).await?;
    Ok(value
        .get("data")
        .and_then(Value::as_array)
        .context("kucoin symbols missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let base = text(row, "baseCurrency").map(kucoin_base);
            let quote = text(row, "quoteCurrency").map(str::to_ascii_uppercase);
            let active = row
                .get("enableTrading")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(market_listing(
                "kucoin",
                "spot",
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                Some(if active { "Trading" } else { "Disabled" }.to_string()),
                None,
                None,
                url,
            ))
        })
        .collect())
}

async fn kucoin_perp_contracts(
    http: &reqwest::Client,
) -> Result<(Vec<MarketListing>, Vec<PerpetualFundingRow>)> {
    let url = "https://api-futures.kucoin.com/api/v1/contracts/active";
    let value = get_json(http, url).await?;
    let mut markets = Vec::new();
    let mut funding = Vec::new();
    for row in value
        .get("data")
        .and_then(Value::as_array)
        .context("kucoin active contracts missing")?
    {
        let native = text(row, "symbol").unwrap_or_default();
        if native.is_empty() {
            continue;
        }
        let base = text(row, "baseCurrency")
            .or_else(|| text(row, "displayBaseCurrency"))
            .map(kucoin_base);
        let quote = text(row, "quoteCurrency").map(str::to_ascii_uppercase);
        let symbol = normalize_pair(base.as_deref(), quote.as_deref(), native);
        let active = text(row, "status").is_none_or(|status| status.eq_ignore_ascii_case("Open"));
        markets.push(market_listing(
            "kucoin",
            "perp",
            symbol.clone(),
            native,
            base,
            quote,
            active,
            text(row, "status").map(str::to_string),
            text(row, "type").map(str::to_string),
            text(row, "settleCurrency").map(str::to_string),
            url,
        ));
        if let Some(funding_rate) = number(row, "fundingFeeRate") {
            funding.push(funding_row(
                "kucoin",
                &symbol,
                native,
                funding_rate,
                time_ms(row, "nextFundingRateTime").or_else(|| time_ms(row, "fundingTime")),
                number(row, "markPrice"),
                number(row, "indexPrice"),
                Some(active),
                url,
                None,
            ));
        }
    }
    Ok((markets, funding))
}

async fn gate_spot_markets(http: &reqwest::Client) -> Result<Vec<MarketListing>> {
    let url = "https://api.gateio.ws/api/v4/spot/currency_pairs";
    let value = get_json(http, url).await?;
    Ok(value
        .as_array()
        .context("gate spot pairs missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "id")?;
            let base = text(row, "base").map(str::to_ascii_uppercase);
            let quote = text(row, "quote").map(str::to_ascii_uppercase);
            let trade_status = text(row, "trade_status").map(str::to_string);
            let active = trade_status.as_deref() == Some("tradable");
            Some(market_listing(
                "gate",
                "spot",
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                trade_status,
                None,
                None,
                url,
            ))
        })
        .collect())
}

async fn gate_perp_contracts(
    http: &reqwest::Client,
) -> Result<(Vec<MarketListing>, Vec<PerpetualFundingRow>)> {
    let url = "https://api.gateio.ws/api/v4/futures/usdt/contracts";
    let value = get_json(http, url).await?;
    let mut markets = Vec::new();
    let mut funding = Vec::new();
    for row in value.as_array().context("gate futures contracts missing")? {
        let native = text(row, "name").unwrap_or_default();
        if native.is_empty() {
            continue;
        }
        let (base, quote) = split_symbol(native);
        let base = base.map(str::to_string);
        let quote = quote.or(Some("USDT")).map(str::to_string);
        let symbol = normalize_pair(base.as_deref(), quote.as_deref(), native);
        let active = !row
            .get("in_delisting")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        markets.push(market_listing(
            "gate",
            "perp",
            symbol.clone(),
            native,
            base,
            quote,
            active,
            Some(if active { "tradable" } else { "delisting" }.to_string()),
            text(row, "type").map(str::to_string),
            Some("USDT".to_string()),
            url,
        ));
        if let Some(funding_rate) = number(row, "funding_rate") {
            funding.push(funding_row(
                "gate",
                &symbol,
                native,
                funding_rate,
                time_ms(row, "funding_next_apply"),
                number(row, "mark_price"),
                number(row, "index_price"),
                Some(active),
                url,
                None,
            ));
        }
    }
    Ok((markets, funding))
}

async fn mexc_spot_markets(http: &reqwest::Client) -> Result<Vec<MarketListing>> {
    let url = "https://api.mexc.com/api/v3/exchangeInfo";
    let value = get_json(http, url).await?;
    Ok(value
        .get("symbols")
        .and_then(Value::as_array)
        .context("mexc spot symbols missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let base = text(row, "baseAsset").map(str::to_ascii_uppercase);
            let quote = text(row, "quoteAsset").map(str::to_ascii_uppercase);
            let status = text(row, "status").map(str::to_string);
            let active =
                status.as_deref() == Some("ENABLED") || status.as_deref() == Some("TRADING");
            Some(market_listing(
                "mexc",
                "spot",
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                status,
                None,
                None,
                url,
            ))
        })
        .collect())
}

async fn mexc_perp_contracts(
    http: &reqwest::Client,
) -> Result<(Vec<MarketListing>, Vec<PerpetualFundingRow>)> {
    let url = "https://contract.mexc.com/api/v1/contract/detail";
    let value = get_json(http, url).await?;
    let mut markets = Vec::new();
    let mut funding = Vec::new();
    for row in value
        .get("data")
        .and_then(Value::as_array)
        .context("mexc contract detail missing")?
    {
        let native = text(row, "symbol").unwrap_or_default();
        if native.is_empty() {
            continue;
        }
        let base = text(row, "baseCoin").map(str::to_ascii_uppercase);
        let quote = text(row, "quoteCoin").map(str::to_ascii_uppercase);
        let symbol = normalize_pair(base.as_deref(), quote.as_deref(), native);
        let active = row
            .get("state")
            .and_then(Value::as_i64)
            .is_none_or(|state| state == 0);
        markets.push(market_listing(
            "mexc",
            "perp",
            symbol.clone(),
            native,
            base,
            quote,
            active,
            row.get("state").map(ToString::to_string),
            Some("perpetual".to_string()),
            text(row, "settleCoin").map(str::to_string),
            url,
        ));
        if let Some(funding_rate) = number(row, "fundingRate") {
            funding.push(funding_row(
                "mexc",
                &symbol,
                native,
                funding_rate,
                time_ms(row, "nextSettleTime"),
                number(row, "fairPrice").or_else(|| number(row, "markPrice")),
                number(row, "indexPrice"),
                Some(active),
                url,
                None,
            ));
        }
    }
    Ok((markets, funding))
}

async fn bingx_perp_contracts(
    http: &reqwest::Client,
) -> Result<(Vec<MarketListing>, Vec<PerpetualFundingRow>)> {
    let contracts_url = "https://open-api.bingx.com/openApi/swap/v2/quote/contracts";
    let premium_url = "https://open-api.bingx.com/openApi/swap/v2/quote/premiumIndex";
    let contracts = get_json(http, contracts_url).await?;
    let premiums = get_json(http, premium_url).await?;
    let mut markets = Vec::new();
    for row in contracts
        .get("data")
        .and_then(Value::as_array)
        .context("bingx contracts missing")?
    {
        let native = text(row, "symbol").unwrap_or_default();
        if native.is_empty() {
            continue;
        }
        let base = text(row, "asset").map(str::to_ascii_uppercase);
        let quote = text(row, "currency").map(str::to_ascii_uppercase);
        let active = row
            .get("status")
            .and_then(Value::as_i64)
            .is_none_or(|status| status == 1);
        markets.push(market_listing(
            "bingx",
            "perp",
            normalize_pair(base.as_deref(), quote.as_deref(), native),
            native,
            base,
            quote,
            active,
            row.get("status").map(ToString::to_string),
            Some("perpetual".to_string()),
            text(row, "currency").map(str::to_string),
            contracts_url,
        ));
    }
    let funding = premiums
        .get("data")
        .and_then(Value::as_array)
        .context("bingx premiumIndex missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let (base, quote) = split_symbol(native);
            let symbol = normalize_pair(base, quote, native);
            let funding_rate = number(row, "lastFundingRate")?;
            Some(funding_row(
                "bingx",
                &symbol,
                native,
                funding_rate,
                time_ms(row, "nextFundingTime"),
                number(row, "markPrice"),
                number(row, "indexPrice"),
                Some(true),
                premium_url,
                u64_value(row, "updateTime"),
            ))
        })
        .collect();
    Ok((markets, funding))
}

async fn bitmart_spot_markets(http: &reqwest::Client) -> Result<Vec<MarketListing>> {
    let url = "https://api-cloud.bitmart.com/spot/v1/symbols/details";
    let value = get_json(http, url).await?;
    Ok(value
        .pointer("/data/symbols")
        .and_then(Value::as_array)
        .context("bitmart spot symbols missing")?
        .iter()
        .filter_map(|row| {
            let native = text(row, "symbol")?;
            let base = text(row, "base_currency").map(str::to_ascii_uppercase);
            let quote = text(row, "quote_currency").map(str::to_ascii_uppercase);
            let status = text(row, "trade_status").map(str::to_string);
            let active = status
                .as_deref()
                .is_none_or(|status| status.eq_ignore_ascii_case("trading"));
            Some(market_listing(
                "bitmart",
                "spot",
                normalize_pair(base.as_deref(), quote.as_deref(), native),
                native,
                base,
                quote,
                active,
                status,
                None,
                None,
                url,
            ))
        })
        .collect())
}

async fn bitmart_perp_details(
    http: &reqwest::Client,
) -> Result<(Vec<MarketListing>, Vec<PerpetualFundingRow>)> {
    let url = "https://api-cloud-v2.bitmart.com/contract/public/details";
    let value = get_json(http, url).await?;
    let mut markets = Vec::new();
    let mut funding = Vec::new();
    for row in value
        .pointer("/data/symbols")
        .and_then(Value::as_array)
        .context("bitmart contract details missing")?
    {
        let native = text(row, "symbol").unwrap_or_default();
        if native.is_empty() {
            continue;
        }
        let base = text(row, "base_currency").map(str::to_ascii_uppercase);
        let quote = text(row, "quote_currency").map(str::to_ascii_uppercase);
        let symbol = normalize_pair(base.as_deref(), quote.as_deref(), native);
        let active = true;
        markets.push(market_listing(
            "bitmart",
            "perp",
            symbol.clone(),
            native,
            base,
            quote,
            active,
            Some("trading".to_string()),
            Some("perpetual".to_string()),
            None,
            url,
        ));
        if let Some(funding_rate) = number(row, "funding_rate") {
            funding.push(funding_row(
                "bitmart",
                &symbol,
                native,
                funding_rate,
                time_ms(row, "funding_time"),
                number(row, "last_price"),
                number(row, "index_price"),
                Some(active),
                url,
                None,
            ));
        }
    }
    Ok((markets, funding))
}

#[allow(clippy::too_many_arguments)]
fn market_listing(
    exchange: &str,
    market: &str,
    symbol: String,
    native_symbol: &str,
    base: Option<String>,
    quote: Option<String>,
    active: bool,
    status: Option<String>,
    contract_type: Option<String>,
    settle_asset: Option<String>,
    source: &str,
) -> MarketListing {
    MarketListing {
        exchange: exchange.to_string(),
        market: market.to_string(),
        symbol,
        native_symbol: native_symbol.to_string(),
        base,
        quote,
        active,
        status,
        contract_type,
        settle_asset,
        source: source.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn funding_row(
    exchange: &str,
    symbol: &str,
    native_symbol: &str,
    funding_rate: f64,
    next_funding_time_ms: Option<u64>,
    mark_price: Option<f64>,
    index_price: Option<f64>,
    active: Option<bool>,
    source: &str,
    ts_ms: Option<u64>,
) -> PerpetualFundingRow {
    PerpetualFundingRow {
        exchange: exchange.to_string(),
        symbol: symbol.to_ascii_uppercase(),
        native_symbol: native_symbol.to_string(),
        funding_rate,
        funding_rate_pct: funding_rate * 100.0,
        next_funding_time_ms,
        mark_price,
        index_price,
        active,
        source: source.to_string(),
        ts_ms: ts_ms.unwrap_or_else(crate::types::now_ms),
    }
}

fn requested_exchanges(
    exchange: Option<&str>,
    exchanges: Option<&str>,
    defaults: &[&str],
) -> Vec<String> {
    let raw = exchange.or(exchanges);
    let values = raw
        .map(|x| {
            x.split(',')
                .map(normalize_lower)
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| defaults.iter().map(|x| x.to_string()).collect());
    values
        .into_iter()
        .filter(|exchange| defaults.iter().any(|supported| *supported == exchange))
        .collect()
}

fn csv_upper_set(input: &str) -> HashSet<String> {
    input
        .split(',')
        .map(|x| x.trim().to_ascii_uppercase())
        .filter(|x| !x.is_empty())
        .collect()
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|x| !x.is_empty())
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|x| x.is_finite())
}

fn u64_value(value: &Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
}

fn time_ms(value: &Value, key: &str) -> Option<u64> {
    let raw = u64_value(value, key)?;
    if raw >= 1_000_000_000_000 {
        Some(raw)
    } else if raw >= 1_000_000_000 {
        Some(raw.saturating_mul(1000))
    } else {
        Some(crate::types::now_ms().saturating_add(raw))
    }
}

fn normalize_lower(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

fn normalize_pair(base: Option<&str>, quote: Option<&str>, fallback: &str) -> String {
    match (base, quote) {
        (Some(base), Some(quote)) => {
            format!("{}{}", normalize_base(base), quote.to_ascii_uppercase())
        }
        _ => fallback
            .replace("-SWAP", "")
            .replace("_SWAP", "")
            .replace("-PERP", "")
            .replace("_PERP", "")
            .replace(['-', '_', '/', ':'], "")
            .to_ascii_uppercase(),
    }
}

fn normalize_base(base: &str) -> String {
    match base.to_ascii_uppercase().as_str() {
        "XBT" => "BTC".to_string(),
        other => other.to_string(),
    }
}

fn binance_base(base: &str) -> String {
    normalize_base(base)
}

fn kucoin_base(base: &str) -> String {
    normalize_base(base)
}

fn okx_base_quote(row: &Value, native: &str) -> (Option<String>, Option<String>) {
    let base = text(row, "baseCcy")
        .filter(|x| !x.is_empty())
        .map(str::to_ascii_uppercase)
        .or_else(|| native.split('-').next().map(normalize_base));
    let quote = text(row, "quoteCcy")
        .filter(|x| !x.is_empty())
        .map(str::to_ascii_uppercase)
        .or_else(|| native.split('-').nth(1).map(str::to_ascii_uppercase));
    (base, quote)
}

fn split_symbol(symbol: &str) -> (Option<&str>, Option<&str>) {
    let clean = symbol
        .strip_suffix("-SWAP")
        .or_else(|| symbol.strip_suffix("_SWAP"))
        .or_else(|| symbol.strip_suffix("-PERP"))
        .or_else(|| symbol.strip_suffix("_PERP"))
        .unwrap_or(symbol);
    if let Some((base, quote)) = clean.split_once('-').or_else(|| clean.split_once('_')) {
        return (Some(base), Some(quote));
    }
    for quote in [
        "USDT", "USDC", "USD", "FDUSD", "BUSD", "TUSD", "BTC", "ETH", "BNB", "KRW", "EUR", "JPY",
        "BRL", "CAD", "AUD",
    ] {
        if let Some(base) = clean.strip_suffix(quote) {
            return (Some(base), Some(quote));
        }
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_native_symbols() {
        assert_eq!(
            normalize_pair(Some("XBT"), Some("USDT"), "XBTUSDTM"),
            "BTCUSDT"
        );
        assert_eq!(normalize_pair(None, None, "BTC-USDT-SWAP"), "BTCUSDT");
        assert_eq!(normalize_pair(None, None, "BTC_USDT"), "BTCUSDT");
    }
}
