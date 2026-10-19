//! Explicit local synthetic source for actual-server acceptance, never a venue.
use crate::source::{ExchangeSource, SourceContext};
use crate::types::*;
use anyhow::{Result, ensure};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureConfig {
    pub events: u64,
    pub events_per_second: u64,
    pub symbols: usize,
}
impl FixtureConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.events > 0 && self.events <= 1_000_000_000_000,
            "fixture events out of range"
        );
        ensure!(
            (1..=1_000_000).contains(&self.events_per_second),
            "fixture rate out of range"
        );
        ensure!(
            (1..=2048).contains(&self.symbols),
            "fixture symbols out of range"
        );
        Ok(())
    }
}
// JSON codec deliberately exercises encode/decode and owned normalization;
// it is a fixture protocol, not a claim of exchange network/adapter validation.
#[derive(Serialize, Deserialize)]
struct Frame {
    symbol: String,
    kind: u8,
    sequence: u64,
    ts_ms: u64,
    price: f64,
    qty: f64,
    buy: bool,
}
fn decode(bytes: &[u8]) -> Result<DataEvent> {
    let f: Frame = serde_json::from_slice(bytes)?;
    let symbol = f.symbol.into_boxed_str();
    Ok(match f.kind {
        0..=5 => DataEvent::Trade(TradeTick {
            exchange: "acceptance_fixture",
            market: MarketKind::Perp,
            symbol,
            price: f.price,
            qty: f.qty,
            side: if f.buy {
                TradeSide::Buy
            } else {
                TradeSide::Sell
            },
            trade_id: Some(f.sequence.to_string().into()),
            ts_ms: f.ts_ms,
        }),
        6..=7 => DataEvent::OrderBook(OrderBookTick {
            exchange: "acceptance_fixture",
            market: MarketKind::Perp,
            symbol,
            bids: (0..10)
                .map(|n| BookLevel {
                    price: f.price - 0.5 - n as f64,
                    qty: 10.0,
                })
                .collect(),
            asks: (0..10)
                .map(|n| BookLevel {
                    price: f.price + 0.5 + n as f64,
                    qty: 10.0,
                })
                .collect(),
            last_update_id: Some(f.sequence),
            ts_ms: f.ts_ms,
        }),
        8 => DataEvent::FundingRate(FundingRateTick {
            exchange: "acceptance_fixture",
            symbol,
            funding_rate: 0.0001,
            mark_price: Some(f.price),
            index_price: Some(f.price),
            next_funding_time_ms: None,
            ts_ms: f.ts_ms,
        }),
        9 => DataEvent::OpenInterest(OpenInterestTick {
            exchange: "acceptance_fixture",
            symbol,
            open_interest: 10000.0,
            open_interest_value: Some(10000.0 * f.price),
            ts_ms: f.ts_ms,
        }),
        _ => anyhow::bail!("invalid fixture frame kind"),
    })
}
pub struct FixtureSource(pub FixtureConfig);
#[async_trait]
impl ExchangeSource for FixtureSource {
    fn name(&self) -> &'static str {
        "acceptance_fixture"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        self.0.validate()?;
        // A restarted generator continues each committed native trade sequence.
        let symbols = (0..self.0.symbols)
            .map(|n| format!("FIX{n:04}USDT"))
            .collect::<Vec<_>>();
        let mut sequences = symbols
            .iter()
            .map(|s| {
                ctx.metrics
                    .ingestion
                    .native_sequence(&format!("acceptance_fixture:trade:perp:{s}"))
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();
        let start = Instant::now();
        for n in 0..self.0.events {
            if n % 32 == 0 {
                let deadline =
                    start + Duration::from_secs_f64(n as f64 / self.0.events_per_second as f64);
                tokio::time::sleep_until(deadline.into()).await;
            }
            let symbol = n as usize % self.0.symbols;
            let kind = ((n / self.0.symbols as u64) % 10) as u8;
            if kind < 6 {
                sequences[symbol] = sequences[symbol]
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("fixture sequence exhausted"))?;
            }
            let received = Instant::now();
            let wall = now_ms();
            let bytes = serde_json::to_vec(&Frame {
                symbol: symbols[symbol].clone(),
                kind,
                sequence: sequences[symbol],
                ts_ms: wall,
                price: 100.0 + symbol as f64,
                qty: 1.0,
                buy: sequences[symbol] % 2 == 1,
            })?;
            ctx.emit_at(decode(&bytes)?, wall, received).await?;
        }
        // Remain supervised and quiet after finite input; never auto-replay it.
        futures_util::future::pending::<()>().await;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_bounds_and_codec_contract() {
        assert!(
            FixtureConfig {
                events: 1,
                events_per_second: 0,
                symbols: 1
            }
            .validate()
            .is_err()
        );
        let f = Frame {
            symbol: "FIX0000USDT".into(),
            kind: 0,
            sequence: 8,
            ts_ms: 123,
            price: 10.0,
            qty: 2.0,
            buy: true,
        };
        let e = decode(&serde_json::to_vec(&f).unwrap()).unwrap();
        assert!(e.has_valid_market_values());
        assert!(
            matches!(e,DataEvent::Trade(t) if t.exchange=="acceptance_fixture" && t.trade_id.as_deref()==Some("8"))
        );
    }
}
