use anyhow::Result;
use serde_json::Value;
use tracing::warn;

use crate::source::SourceContext;
use crate::types::{BookLevel, DataEvent, MarketKind, MarketTick, TradeSide, now_ms};

pub async fn emit_tick(
    ctx: &SourceContext,
    exchange: &'static str,
    market: MarketKind,
    symbol: &str,
    bid: &str,
    ask: &str,
) -> Result<()> {
    emit_tick_ext(ctx, exchange, market, symbol, bid, ask, None, None, None).await
}

#[allow(clippy::too_many_arguments)]
pub async fn emit_tick_ext(
    ctx: &SourceContext,
    exchange: &'static str,
    market: MarketKind,
    symbol: &str,
    bid: &str,
    ask: &str,
    mark: Option<&str>,
    funding_rate: Option<&str>,
    source_ts_ms: Option<u64>,
) -> Result<()> {
    let parsed_bid = bid.parse::<f64>();
    let parsed_ask = ask.parse::<f64>();

    if let (Ok(bid), Ok(ask)) = (parsed_bid, parsed_ask) {
        let mark = mark.and_then(|x| x.parse::<f64>().ok());
        let funding_rate = funding_rate.and_then(|x| x.parse::<f64>().ok());
        let tick = MarketTick {
            exchange,
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark,
            funding_rate,
            ts_ms: source_ts_ms.unwrap_or_else(now_ms),
        };
        ctx.emit(DataEvent::Tick(tick)).await?;
    } else {
        warn!(exchange, symbol, bid_raw=%bid, ask_raw=%ask, "invalid tick parse");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn emit_tick_f64(
    ctx: &SourceContext,
    exchange: &'static str,
    market: MarketKind,
    symbol: &str,
    bid: f64,
    ask: f64,
    mark: Option<f64>,
    funding_rate: Option<f64>,
    source_ts_ms: Option<u64>,
) -> Result<()> {
    if bid > 0.0 && ask > 0.0 && bid.is_finite() && ask.is_finite() {
        let tick = MarketTick {
            exchange,
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark,
            funding_rate,
            ts_ms: source_ts_ms.unwrap_or_else(now_ms),
        };
        ctx.emit(DataEvent::Tick(tick)).await?;
    }
    Ok(())
}

pub fn first_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| match value.get(*key)? {
        Value::String(text) => Some(text.as_str()),
        _ => None,
    })
}

pub fn parse_value_f64(value: &Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|x| x.parse::<f64>().ok())
        .or_else(|| value.as_f64())
}

pub fn parse_array_levels(items: &[Value]) -> Vec<BookLevel> {
    items
        .iter()
        .filter_map(|item| {
            let pair = item.as_array()?;
            Some(BookLevel {
                price: parse_value_f64(pair.first()?)?,
                qty: parse_value_f64(pair.get(1)?)?,
            })
        })
        .collect()
}

pub fn parse_object_levels(items: &[Value], price_key: &str, qty_key: &str) -> Vec<BookLevel> {
    items
        .iter()
        .filter_map(|item| {
            Some(BookLevel {
                price: parse_value_f64(item.get(price_key)?)?,
                qty: parse_value_f64(item.get(qty_key)?)?,
            })
        })
        .collect()
}

pub fn side_from_labels(side: &str, buy_labels: &[&str], sell_labels: &[&str]) -> TradeSide {
    if buy_labels
        .iter()
        .any(|label| side.eq_ignore_ascii_case(label))
    {
        TradeSide::Buy
    } else if sell_labels
        .iter()
        .any(|label| side.eq_ignore_ascii_case(label))
    {
        TradeSide::Sell
    } else {
        TradeSide::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{first_str, parse_array_levels, parse_object_levels, side_from_labels};
    use crate::types::TradeSide;
    use serde_json::json;

    #[test]
    fn shared_parsers_accept_common_exchange_shapes() {
        let value = json!({"bid": "1.2", "ask": 1.3});
        assert_eq!(first_str(&value, &["bid"]), Some("1.2"));
        assert_eq!(
            parse_array_levels(&[json!(["100.5", "2"]), json!([99.5, 3.0])]).len(),
            2
        );
        assert_eq!(
            parse_object_levels(&[json!({"px": "100", "sz": "4"})], "px", "sz")[0].qty,
            4.0
        );
        assert_eq!(
            side_from_labels("BUY", &["buy", "b"], &["sell", "s"]),
            TradeSide::Buy
        );
    }
}
