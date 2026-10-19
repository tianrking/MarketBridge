//! Scoped reconstruction from ordered native trades. A finite-window proof
//! never clears a process-global gap or repairs lifetime cumulative CVD.
use crate::types::TradeSide;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WindowRequest {
    pub exchange: String,
    pub market: String,
    pub symbol: String,
    pub start_ms: u64,
    pub end_ms: u64,
}
impl WindowRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.exchange.as_str(), "binance" | "acceptance_fixture"),
            "native window recovery supports Binance aggTrade and explicit acceptance fixtures only"
        );
        ensure!(
            matches!(self.market.as_str(), "spot" | "perp")
                && !self.symbol.is_empty()
                && self.symbol.len() <= 128,
            "invalid window instrument"
        );
        ensure!(
            self.start_ms > 0 && self.start_ms < self.end_ms,
            "window must be a positive half-open [start_ms,end_ms) interval"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowTrade {
    pub sequence: u64,
    pub ts_ms: u64,
    pub price: f64,
    pub qty: f64,
    pub side: TradeSide,
}
impl WindowTrade {
    fn valid(&self) -> bool {
        self.ts_ms > 0
            && self.price.is_finite()
            && self.price > 0.0
            && self.qty.is_finite()
            && self.qty > 0.0
            && (self.price * self.qty).is_finite()
            && self.side != TradeSide::Unknown
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WindowTotals {
    pub trade_count: u64,
    pub buy_qty: f64,
    pub sell_qty: f64,
    pub buy_notional: f64,
    pub sell_notional: f64,
    pub delta_qty: f64,
    pub delta_notional: f64,
    pub cvd_anchor: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowProof {
    pub verified: bool,
    pub status: &'static str,
    pub scope: WindowRequest,
    pub before_sequence: Option<u64>,
    pub after_sequence: Option<u64>,
    pub canonicalized_duplicates: u64,
    pub evidence_crc32: u32,
    pub lifetime_cvd_repaired: bool,
    pub upstream_assumption: &'static str,
}
#[derive(Debug, Serialize)]
pub struct WindowRecovery {
    pub quality: WindowProof,
    pub recovered: Option<WindowTotals>,
}

pub struct WindowAccumulator {
    scope: WindowRequest,
    before: Option<WindowTrade>,
    previous: Option<WindowTrade>,
    after: Option<WindowTrade>,
    totals: WindowTotals,
    duplicates: u64,
    failure: Option<&'static str>,
    crc: flate2::Crc,
    started: bool,
}
impl WindowAccumulator {
    pub fn new(scope: WindowRequest) -> Result<Self> {
        scope.validate()?;
        let mut crc = flate2::Crc::new();
        crc.update(&serde_json::to_vec(&scope)?);
        Ok(Self {
            scope,
            before: None,
            previous: None,
            after: None,
            totals: WindowTotals {
                cvd_anchor: "window_start_only",
                ..Default::default()
            },
            duplicates: 0,
            failure: None,
            crc,
            started: false,
        })
    }
    pub fn push(&mut self, trade: WindowTrade) {
        if self.after.is_some() || self.failure.is_some() {
            return;
        }
        if !trade.valid() {
            self.failure = Some("invalid_trade_values_or_direction");
            return;
        }
        // Earlier gaps cannot taint an independently bounded later window.
        if trade.ts_ms < self.scope.start_ms {
            if self.started {
                self.failure = Some("source_time_regression");
                return;
            }
            self.crc = flate2::Crc::new();
            self.crc
                .update(&serde_json::to_vec(&self.scope).expect("serializable scope"));
            self.hash_trade(trade);
            self.before = Some(trade);
            self.previous = Some(trade);
            return;
        }
        self.started = true;
        let Some(previous) = self.previous else {
            self.failure = Some("missing_before_boundary");
            return;
        };
        if trade.sequence == previous.sequence {
            if trade == previous {
                self.duplicates += 1;
                return;
            }
            self.failure = Some("native_identity_content_conflict");
            return;
        }
        if previous.sequence.checked_add(1) != Some(trade.sequence) {
            self.failure = Some("native_sequence_gap_or_regression");
            return;
        }
        if trade.ts_ms < previous.ts_ms {
            self.failure = Some("source_time_regression");
            return;
        }
        self.previous = Some(trade);
        self.hash_trade(trade);
        if trade.ts_ms >= self.scope.end_ms {
            self.after = Some(trade);
            return;
        }
        self.totals.trade_count += 1;
        let notional = trade.price * trade.qty;
        match trade.side {
            TradeSide::Buy => {
                self.totals.buy_qty += trade.qty;
                self.totals.buy_notional += notional;
            }
            TradeSide::Sell => {
                self.totals.sell_qty += trade.qty;
                self.totals.sell_notional += notional;
            }
            TradeSide::Unknown => unreachable!(),
        }
        self.totals.delta_qty = self.totals.buy_qty - self.totals.sell_qty;
        self.totals.delta_notional = self.totals.buy_notional - self.totals.sell_notional;
        if [
            self.totals.buy_qty,
            self.totals.sell_qty,
            self.totals.buy_notional,
            self.totals.sell_notional,
            self.totals.delta_qty,
            self.totals.delta_notional,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            self.failure = Some("aggregate_arithmetic_overflow");
        }
    }
    fn hash_trade(&mut self, trade: WindowTrade) {
        self.crc.update(&trade.sequence.to_le_bytes());
        self.crc.update(&trade.ts_ms.to_le_bytes());
        self.crc.update(&trade.price.to_bits().to_le_bytes());
        self.crc.update(&trade.qty.to_bits().to_le_bytes());
        self.crc
            .update(&[if trade.side == TradeSide::Buy { 1 } else { 2 }]);
    }
    pub fn reject(&mut self, status: &'static str) {
        self.failure = Some(status);
    }
    pub fn finish(self) -> WindowRecovery {
        let status = self.failure.unwrap_or(if self.before.is_none() {
            "missing_before_boundary"
        } else if self.after.is_none() {
            "open_or_missing_after_boundary"
        } else {
            "verified_native_window"
        });
        let verified = status == "verified_native_window";
        WindowRecovery {
            quality: WindowProof {
                verified,
                status,
                scope: self.scope,
                before_sequence: self.before.map(|p| p.sequence),
                after_sequence: self.after.map(|p| p.sequence),
                canonicalized_duplicates: self.duplicates,
                evidence_crc32: self.crc.sum(),
                lifetime_cvd_repaired: false,
                upstream_assumption: "ordered immutable native aggregate-trade IDs and source timestamps; normalized evidence is not exchange attestation",
            },
            recovered: verified.then_some(self.totals),
        }
    }
}

/// Streams sealed normalized evidence. Global issues stay visible; only the
/// native bounded segment is certified, never the entire journal.
pub fn recover_journal(path: &std::path::Path, scope: WindowRequest) -> Result<serde_json::Value> {
    let mut accumulator = WindowAccumulator::new(scope.clone())?;
    let verification = crate::journal::visit(path, |record| {
        let p = &record.payload;
        if p["type"] == "trade"
            && p["exchange"] == scope.exchange
            && p["market"] == scope.market
            && p["symbol"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&scope.symbol))
        {
            let trade = (|| {
                Some(WindowTrade {
                    sequence: p["trade_id"].as_str()?.parse().ok()?,
                    ts_ms: p["ts_ms"].as_u64()?,
                    price: p["price"].as_f64()?,
                    qty: p["qty"].as_f64()?,
                    side: match p["side"].as_str()? {
                        "buy" => TradeSide::Buy,
                        "sell" => TradeSide::Sell,
                        _ => TradeSide::Unknown,
                    },
                })
            })();
            if let Some(trade) = trade {
                accumulator.push(trade);
            } else {
                accumulator.reject("invalid_native_trade_evidence");
            }
        }
        Ok(())
    })?;
    ensure!(
        verification.sealed,
        "window recovery requires a sealed CRC-valid journal"
    );
    Ok(
        serde_json::json!({"model_version":"native-trade-window/v1","journal":verification,"window":accumulator.finish(),"global_quality_cleared":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> WindowRequest {
        WindowRequest {
            exchange: "binance".into(),
            market: "perp".into(),
            symbol: "BTCUSDT".into(),
            start_ms: 100,
            end_ms: 200,
        }
    }
    fn trade(sequence: u64, ts_ms: u64) -> WindowTrade {
        WindowTrade {
            sequence,
            ts_ms,
            price: 10.0,
            qty: 2.0,
            side: TradeSide::Buy,
        }
    }
    fn run(rows: &[WindowTrade]) -> WindowRecovery {
        let mut a = WindowAccumulator::new(scope()).unwrap();
        for t in rows {
            a.push(*t);
        }
        a.finish()
    }
    #[test]
    fn boundaries_deduplicate_and_recover_exact_window() {
        let r = run(&[
            trade(1, 1),
            trade(10, 90),
            trade(11, 100),
            trade(11, 100),
            WindowTrade {
                side: TradeSide::Sell,
                ..trade(12, 150)
            },
            trade(13, 200),
            trade(99, 201),
        ]);
        assert!(r.quality.verified);
        assert_eq!(r.quality.canonicalized_duplicates, 1);
        let totals = r.recovered.unwrap();
        assert_eq!(totals.trade_count, 2);
        assert_eq!(totals.delta_qty, 0.0);
        assert_eq!(totals.buy_notional, 20.0);
        assert!(!r.quality.lifetime_cvd_repaired);
    }
    #[test]
    fn missing_boundaries_and_internal_gap_fail_closed() {
        for rows in [
            vec![trade(1, 100), trade(2, 200)],
            vec![trade(1, 90), trade(2, 100)],
            vec![trade(1, 90), trade(3, 100), trade(4, 200)],
        ] {
            let r = run(&rows);
            assert!(!r.quality.verified);
            assert!(r.recovered.is_none());
        }
    }
    #[test]
    fn collisions_time_regressions_and_overflow_fail_closed() {
        for rows in [
            vec![trade(1, 90), trade(2, 120), trade(2, 121), trade(3, 200)],
            vec![trade(1, 90), trade(2, 150), trade(3, 120), trade(4, 200)],
            vec![trade(1, 90), trade(2, 120), trade(3, 80), trade(4, 200)],
            vec![
                trade(1, 90),
                WindowTrade {
                    price: f64::MAX / 2.0,
                    qty: 1.0,
                    ..trade(2, 100)
                },
                WindowTrade {
                    price: f64::MAX / 2.0,
                    qty: 1.0,
                    ..trade(3, 110)
                },
                WindowTrade {
                    price: f64::MAX / 2.0,
                    qty: 1.0,
                    ..trade(4, 120)
                },
                trade(5, 200),
            ],
        ] {
            assert!(run(&rows).recovered.is_none());
        }
    }
    #[test]
    fn zero_trade_window_and_boundary_checksum() {
        let r = run(&[trade(10, 90), trade(11, 200)]);
        assert_eq!(r.recovered.unwrap().trade_count, 0);
        let other = run(&[
            WindowTrade {
                qty: 3.0,
                ..trade(10, 90)
            },
            trade(11, 200),
        ]);
        assert_ne!(r.quality.evidence_crc32, other.quality.evidence_crc32);
        assert!(
            WindowAccumulator::new(WindowRequest {
                exchange: "unknown".into(),
                ..scope()
            })
            .is_err()
        );
    }
}
