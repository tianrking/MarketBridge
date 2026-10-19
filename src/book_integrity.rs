//! Bounded L2 reconstruction. Exact decimal keys and original wire lexemes are
//! retained: f64 is an output compatibility format, never a book identity key.
use crate::types::{BookLevel, MarketKind, OrderBookTick};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct BookConfig {
    pub max_symbols: usize,
    pub max_state_bytes: usize,
    pub max_total_buffered_bytes: usize,
    pub max_levels_per_side: usize,
    pub publish_levels: usize,
    pub bybit_depth: usize,
    pub max_buffered_updates: usize,
    pub max_buffered_bytes: usize,
    pub snapshot_timeout_ms: u64,
    pub max_age_ms: u64,
    pub binance_weight_per_minute: u64,
}
impl Default for BookConfig {
    fn default() -> Self {
        Self {
            max_symbols: 256,
            max_state_bytes: 64 * 1024 * 1024,
            max_total_buffered_bytes: 64 * 1024 * 1024,
            max_levels_per_side: 1000,
            publish_levels: 200,
            bybit_depth: 1000,
            max_buffered_updates: 256,
            max_buffered_bytes: 4 * 1024 * 1024,
            snapshot_timeout_ms: 10000,
            max_age_ms: 5000,
            binance_weight_per_minute: 1200,
        }
    }
}
impl BookConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=4096).contains(&self.max_symbols)
                && (1000..=5000).contains(&self.max_levels_per_side)
                && self.publish_levels > 0
                && self.publish_levels <= self.max_levels_per_side
                && self.publish_levels <= 1000
                && matches!(self.bybit_depth, 50 | 200 | 1000)
                && self.max_state_bytes >= 65536
                && self.max_total_buffered_bytes >= 65536,
            "invalid order book symbol/depth budgets"
        );
        ensure!(
            (1..=4096).contains(&self.max_buffered_updates)
                && (65536..=64 * 1024 * 1024).contains(&self.max_buffered_bytes)
                && (100..=60000).contains(&self.snapshot_timeout_ms)
                && self.max_age_ms > 0
                && (50..=2400).contains(&self.binance_weight_per_minute),
            "invalid order book buffering/time/rate budgets"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    BinanceSpot,
    BinancePerp,
    Okx,
    Bybit,
}
impl Protocol {
    pub fn exchange(self) -> &'static str {
        match self {
            Self::BinanceSpot | Self::BinancePerp => "binance",
            Self::Okx => "okx",
            Self::Bybit => "bybit",
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct BookQuality {
    #[serde(skip)]
    pub reconstructed_at: Instant,
    pub version: &'static str,
    pub synchronized: bool,
    pub session: String,
    pub revision: u64,
    pub protocol: Protocol,
    pub snapshot_sequence: u64,
    pub snapshot_generation: u64,
    pub native_sequence: u64,
    pub sequence_policy: &'static str,
    pub checksum_policy: &'static str,
    pub server_checksum: Option<i32>,
    pub evidence_crc32: u32,
    pub quantity_unit: &'static str,
    pub native_quantity_unit: &'static str,
    pub base_quantity_multiplier: f64,
    pub coverage: &'static str,
    pub subscription_depth: usize,
    pub retained_bids: usize,
    pub retained_asks: usize,
    pub exact_bids: Vec<[String; 2]>,
    pub exact_asks: Vec<[String; 2]>,
}
impl BookQuality {
    pub fn allocated_bytes(&self) -> usize {
        self.session.capacity()
            + [&self.exact_bids, &self.exact_asks]
                .iter()
                .map(|rows| {
                    rows.capacity() * std::mem::size_of::<[String; 2]>()
                        + rows.iter().flatten().map(String::capacity).sum::<usize>()
                })
                .sum::<usize>()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireLevel {
    pub price: String,
    pub qty: String,
}
impl WireLevel {
    pub fn new(price: &str, qty: &str) -> Self {
        Self {
            price: price.into(),
            qty: qty.into(),
        }
    }
}
// At most 20 integer +18 fractional digits, checked u128 arithmetic. No exponent
// notation, NaN, negative values, rounding, or float-key aliasing is accepted.
fn decimal(value: &str) -> Result<u128> {
    ensure!(
        !value.is_empty() && value.len() <= 40,
        "decimal length invalid"
    );
    let mut parts = value.split('.');
    let whole = parts.next().unwrap();
    let fraction = parts.next().unwrap_or("");
    ensure!(
        parts.next().is_none()
            && !whole.is_empty()
            && fraction.len() <= 18
            && whole.bytes().all(|b| b.is_ascii_digit())
            && fraction.bytes().all(|b| b.is_ascii_digit()),
        "invalid exact decimal"
    );
    let whole = whole.parse::<u128>()?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u128>()? * 10u128.pow((18 - fraction.len()) as u32)
    };
    whole
        .checked_mul(10u128.pow(18))
        .and_then(|x| x.checked_add(fraction))
        .ok_or_else(|| anyhow::anyhow!("decimal overflow"))
}

#[derive(Debug, Clone)]
pub struct Update {
    pub first: u64,
    pub last: u64,
    pub previous: Option<u64>,
    pub ts_ms: u64,
    pub snapshot: bool,
    pub bids: Vec<WireLevel>,
    pub asks: Vec<WireLevel>,
    pub checksum: Option<i32>,
}
#[derive(Clone)]
pub struct LocalBook {
    protocol: Protocol,
    market: MarketKind,
    symbol: String,
    session: String,
    bids: BTreeMap<u128, WireLevel>,
    asks: BTreeMap<u128, WireLevel>,
    sequence: Option<u64>,
    snapshot_sequence: u64,
    snapshot_generation: u64,
    revision: u64,
    bridged: bool,
    bid_floor: Option<u128>,
    ask_ceiling: Option<u128>,
    last_ts: u64,
    last_fingerprint: Option<u32>,
    last_cross_sequence: Option<u64>,
    quantity_multiplier: Option<f64>,
    config: BookConfig,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    Ignored,
    Publish,
}
impl LocalBook {
    pub fn new(
        protocol: Protocol,
        market: MarketKind,
        symbol: String,
        session: String,
        config: BookConfig,
    ) -> Self {
        Self {
            protocol,
            market,
            symbol,
            session,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            sequence: None,
            snapshot_sequence: 0,
            snapshot_generation: 0,
            revision: 0,
            bridged: false,
            bid_floor: None,
            ask_ceiling: None,
            last_ts: 0,
            last_fingerprint: None,
            last_cross_sequence: None,
            quantity_multiplier: if protocol == Protocol::Okx && market == MarketKind::Perp {
                None
            } else {
                Some(1.0)
            },
            config,
        }
    }
    pub fn apply(&mut self, u: &Update) -> Result<Applied> {
        let result = self.apply_inner(u);
        if result.is_err() {
            self.sequence = None;
            self.bridged = false;
            self.bids.clear();
            self.asks.clear();
        }
        result
    }
    fn apply_inner(&mut self, u: &Update) -> Result<Applied> {
        ensure!(
            u.first <= u.last && u.ts_ms > 0,
            "invalid sequence range or timestamp"
        );
        ensure!(
            u.bids.len() <= self.config.max_levels_per_side * 2
                && u.asks.len() <= self.config.max_levels_per_side * 2,
            "wire update exceeds level budget"
        );
        let fingerprint = update_crc(u);
        if u.snapshot {
            ensure!(
                self.protocol != Protocol::Okx || u.previous.is_none(),
                "OKX snapshot prevSeqId must be -1"
            );
        } else {
            let last = self
                .sequence
                .ok_or_else(|| anyhow::anyhow!("delta before snapshot"))?;
            if matches!(self.protocol, Protocol::BinanceSpot | Protocol::BinancePerp)
                && (u.last < last
                    || (u.last == last && (self.protocol != Protocol::BinancePerp || self.bridged)))
            {
                return Ok(Applied::Ignored);
            }
            if self.protocol == Protocol::Bybit && u.last == last {
                ensure!(
                    self.last_fingerprint == Some(fingerprint),
                    "Bybit duplicate identity content conflict"
                );
                return Ok(Applied::Ignored);
            }
            match self.protocol {
                Protocol::BinanceSpot => ensure!(
                    u.first
                        <= last
                            .checked_add(1)
                            .ok_or_else(|| anyhow::anyhow!("sequence overflow"))?
                        && u.last > last,
                    "Binance spot sequence gap"
                ),
                Protocol::BinancePerp => {
                    if self.bridged {
                        ensure!(u.previous == Some(last), "Binance perp pu gap");
                    } else {
                        ensure!(
                            u.first <= last && u.last >= last,
                            "Binance perp snapshot bridge unavailable"
                        );
                    }
                }
                Protocol::Okx => {
                    ensure!(u.previous == Some(last), "OKX prevSeqId mismatch");
                    if u.last == last {
                        ensure!(
                            u.bids.is_empty() && u.asks.is_empty(),
                            "OKX same-sequence update changes book"
                        );
                    }
                }
                Protocol::Bybit => {
                    ensure!(u.last > last, "Bybit sequence regression");
                    ensure!(
                        u.previous.is_none()
                            || self
                                .last_cross_sequence
                                .is_none_or(|old| u.previous.is_some_and(|next| next >= old)),
                        "Bybit cross-sequence regression"
                    );
                }
            }
            ensure!(u.ts_ms >= self.last_ts, "book source time regression");
        }
        // Move private bounded maps; invalid fields/checksum clear the state
        // before any reconstructed view can be published.
        let mut bids = if u.snapshot {
            BTreeMap::new()
        } else {
            std::mem::take(&mut self.bids)
        };
        let mut asks = if u.snapshot {
            BTreeMap::new()
        } else {
            std::mem::take(&mut self.asks)
        };
        apply_side(&mut bids, &u.bids, u.snapshot)?;
        apply_side(&mut asks, &u.asks, u.snapshot)?;
        let retained_limit = match self.protocol {
            Protocol::Bybit => self.config.bybit_depth,
            Protocol::Okx => 400,
            _ => self.config.max_levels_per_side,
        };
        if u.snapshot {
            ensure!(
                u.bids.len() <= retained_limit && u.asks.len() <= retained_limit,
                "snapshot exceeds subscribed depth"
            );
        }
        trim(&mut bids, retained_limit, true);
        trim(&mut asks, retained_limit, false);
        ensure!(!bids.is_empty() && !asks.is_empty(), "empty book side");
        ensure!(
            bids.last_key_value().unwrap().0 < asks.first_key_value().unwrap().0,
            "crossed or locked book"
        );
        if self.protocol == Protocol::Okx
            && let Some(expected) = u.checksum
        {
            ensure!(
                okx_checksum(&bids, &asks) == expected,
                "OKX checksum mismatch"
            );
        }
        let is_binance = matches!(self.protocol, Protocol::BinanceSpot | Protocol::BinancePerp);
        let bid_floor = Some(
            (if u.snapshot {
                0
            } else {
                self.bid_floor.unwrap_or(0)
            })
            .max(*bids.first_key_value().unwrap().0),
        );
        let ask_ceiling = Some(if u.snapshot {
            *asks.last_key_value().unwrap().0
        } else {
            self.ask_ceiling
                .unwrap_or(u128::MAX)
                .min(*asks.last_key_value().unwrap().0)
        });
        if is_binance && !u.snapshot {
            ensure!(
                bids.last_key_value()
                    .is_some_and(|(p, _)| Some(*p) >= bid_floor)
                    && asks
                        .first_key_value()
                        .is_some_and(|(p, _)| Some(*p) <= ask_ceiling),
                "initial snapshot coverage exhausted"
            );
        }
        self.bids = bids;
        self.asks = asks;
        self.bid_floor = bid_floor;
        self.ask_ceiling = ask_ceiling;
        if u.snapshot {
            self.snapshot_sequence = u.last;
            self.snapshot_generation += 1;
            self.bridged = !is_binance;
        } else {
            self.bridged = true;
        }
        self.sequence = Some(u.last);
        self.last_ts = if u.snapshot && is_binance { 0 } else { u.ts_ms };
        self.last_fingerprint = Some(fingerprint);
        self.last_cross_sequence = u.previous;
        self.revision += 1;
        Ok(if self.bridged {
            Applied::Publish
        } else {
            Applied::Ignored
        })
    }
    pub fn set_contract_value(&mut self, value: f64) -> Result<()> {
        ensure!(value.is_finite() && value > 0.0, "invalid contract value");
        self.quantity_multiplier = Some(value);
        Ok(())
    }
    pub fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.symbol.capacity()
            + self.session.capacity()
            + self
                .bids
                .values()
                .chain(self.asks.values())
                .map(|l| {
                    std::mem::size_of::<WireLevel>() + 48 + l.price.capacity() + l.qty.capacity()
                })
                .sum::<usize>()
    }
    pub fn tick(&self, checksum: Option<i32>) -> Result<OrderBookTick> {
        ensure!(
            self.bridged && self.sequence.is_some(),
            "book is not synchronized"
        );
        let mut bids = self.bids.iter().rev().collect::<Vec<_>>();
        let mut asks = self.asks.iter().collect::<Vec<_>>();
        // Binance REST bounds known depth. Do not publish cached levels beyond
        // the original snapshot fence as if unknown intermediate levels exist.
        if matches!(self.protocol, Protocol::BinanceSpot | Protocol::BinancePerp) {
            bids.retain(|(p, _)| self.bid_floor.is_some_and(|f| **p >= f));
            asks.retain(|(p, _)| self.ask_ceiling.is_some_and(|c| **p <= c));
        }
        let exact_bids = bids
            .into_iter()
            .take(self.config.publish_levels)
            .map(|(_, l)| [l.price.clone(), l.qty.clone()])
            .collect::<Vec<_>>();
        let exact_asks = asks
            .into_iter()
            .take(self.config.publish_levels)
            .map(|(_, l)| [l.price.clone(), l.qty.clone()])
            .collect::<Vec<_>>();
        let multiplier = self
            .quantity_multiplier
            .context("perpetual contract units not verified")?;
        let project = |rows: &Vec<[String; 2]>, bid: bool| -> Result<Vec<BookLevel>> {
            let levels = rows
                .iter()
                .map(|[p, q]| {
                    Ok(BookLevel {
                        price: p.parse()?,
                        qty: q.parse::<f64>()? * multiplier,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                levels.iter().all(|l| l.price.is_finite()
                    && l.price > 0.0
                    && l.qty.is_finite()
                    && l.qty > 0.0)
                    && levels.windows(2).all(|w| if bid {
                        w[0].price > w[1].price
                    } else {
                        w[0].price < w[1].price
                    }),
                "f64 output cannot represent exact ordered levels"
            );
            Ok(levels)
        };
        let bids = project(&exact_bids, true)?;
        let asks = project(&exact_asks, false)?;
        ensure!(
            bids[0].price < asks[0].price,
            "f64 output aliases opposite book sides"
        );
        let bytes = serde_json::to_vec(&(
            &self.session,
            self.revision,
            self.sequence,
            multiplier,
            &exact_bids,
            &exact_asks,
        ))?;
        let mut crc = flate2::Crc::new();
        crc.update(&bytes);
        Ok(OrderBookTick {
            exchange: self.protocol.exchange(),
            market: self.market,
            symbol: self.symbol.clone().into(),
            bids,
            asks,
            last_update_id: self.sequence,
            ts_ms: self.last_ts,
            book_quality: Some(BookQuality {
                reconstructed_at: Instant::now(),
                version: "l2-reconstruction/v1",
                synchronized: true,
                session: self.session.clone(),
                revision: self.revision,
                protocol: self.protocol,
                snapshot_sequence: self.snapshot_sequence,
                snapshot_generation: self.snapshot_generation,
                native_sequence: self.sequence.unwrap(),
                sequence_policy: match self.protocol {
                    Protocol::BinanceSpot => "overlapping_U_u_covers_next",
                    Protocol::BinancePerp => "snapshot_U_u_bridge_then_pu_chain",
                    Protocol::Okx => "prevSeqId_chain_allows_documented_reset",
                    Protocol::Bybit => "ordered_connection_monotonic_u_no_gap_attestation",
                },
                checksum_policy: if self.protocol == Protocol::Okx && checksum.is_some() {
                    "server_crc32_top25_verified"
                } else {
                    "server_checksum_not_provided"
                },
                server_checksum: checksum,
                evidence_crc32: crc.sum(),
                quantity_unit: "base_asset",
                native_quantity_unit: if self.protocol == Protocol::Okx
                    && self.market == MarketKind::Perp
                {
                    "contracts"
                } else {
                    "base_asset"
                },
                base_quantity_multiplier: multiplier,
                coverage: if matches!(self.protocol, Protocol::BinanceSpot | Protocol::BinancePerp)
                {
                    "bounded_initial_snapshot_price_range"
                } else {
                    "subscribed_depth_only"
                },
                subscription_depth: match self.protocol {
                    Protocol::Bybit => self.config.bybit_depth,
                    Protocol::Okx => 400,
                    _ => 1000,
                },
                retained_bids: self.bids.len(),
                retained_asks: self.asks.len(),
                exact_bids,
                exact_asks,
            }),
        })
    }
}
fn apply_side(
    side: &mut BTreeMap<u128, WireLevel>,
    rows: &[WireLevel],
    snapshot: bool,
) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for row in rows {
        let p = decimal(&row.price)?;
        let q = decimal(&row.qty)?;
        ensure!(
            p > 0 && seen.insert(p),
            "invalid or duplicate exact price in frame"
        );
        if q == 0 {
            ensure!(!snapshot, "snapshot contains zero size");
            side.remove(&p);
        } else {
            side.insert(p, row.clone());
        }
    }
    Ok(())
}
fn trim(side: &mut BTreeMap<u128, WireLevel>, n: usize, bids: bool) {
    while side.len() > n {
        if bids {
            side.pop_first();
        } else {
            side.pop_last();
        }
    }
}
fn okx_checksum(bids: &BTreeMap<u128, WireLevel>, asks: &BTreeMap<u128, WireLevel>) -> i32 {
    let b = bids.values().rev().take(25).collect::<Vec<_>>();
    let a = asks.values().take(25).collect::<Vec<_>>();
    let mut fields = Vec::new();
    for n in 0..b.len().max(a.len()) {
        if let Some(l) = b.get(n) {
            fields.push(l.price.as_str());
            fields.push(l.qty.as_str());
        }
        if let Some(l) = a.get(n) {
            fields.push(l.price.as_str());
            fields.push(l.qty.as_str());
        }
    }
    let mut crc = flate2::Crc::new();
    crc.update(fields.join(":").as_bytes());
    crc.sum() as i32
}
fn update_crc(u: &Update) -> u32 {
    let mut c = flate2::Crc::new();
    for n in [u.first, u.last, u.previous.unwrap_or(u64::MAX), u.ts_ms] {
        c.update(&n.to_le_bytes());
    }
    for side in [&u.bids, &u.asks] {
        c.update(&[255]);
        for l in side {
            c.update(l.price.as_bytes());
            c.update(&[0]);
            c.update(l.qty.as_bytes());
            c.update(&[0]);
        }
    }
    c.sum()
}

#[derive(Clone)]
pub struct BookRegistry {
    inner: Arc<Mutex<RegistryState>>,
}
struct RegistryState {
    config: BookConfig,
    rows: BTreeMap<String, BookStatus>,
    next: u64,
    next_request: Instant,
    blocked_until: Instant,
    bytes: usize,
    buffered_bytes: usize,
}
#[derive(Clone, Serialize)]
struct BookStatus {
    session: String,
    revision: u64,
    phase: &'static str,
    reason: Option<&'static str>,
    resyncs: u64,
    bytes: usize,
    buffered_bytes: usize,
    #[serde(skip)]
    seen: Option<Instant>,
}
pub struct BookLease {
    registry: BookRegistry,
    key: String,
    pub session: String,
}
impl Default for BookRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RegistryState {
                config: BookConfig::default(),
                rows: BTreeMap::new(),
                next: 0,
                next_request: Instant::now(),
                blocked_until: Instant::now(),
                bytes: 0,
                buffered_bytes: 0,
            })),
        }
    }
}
impl BookRegistry {
    pub fn configure(&self, config: BookConfig) {
        self.inner.lock().unwrap().config = config;
    }
    pub fn config(&self) -> BookConfig {
        self.inner.lock().unwrap().config.clone()
    }
    pub fn begin(&self, exchange: &str, market: MarketKind, symbol: &str) -> Result<BookLease> {
        let key = book_key(exchange, market, symbol);
        let mut s = self.inner.lock().unwrap();
        ensure!(
            s.rows.contains_key(&key) || s.rows.len() < s.config.max_symbols,
            "global reconstructed book symbol budget exceeded"
        );
        s.next += 1;
        let session = format!(
            "{}:{}",
            crate::outbox::EventIdentity::process_origin(),
            s.next
        );
        let resyncs = s.rows.get(&key).map_or(0, |r| r.resyncs + 1);
        if let Some(old) = s.rows.remove(&key) {
            s.bytes -= old.bytes;
            s.buffered_bytes -= old.buffered_bytes;
        }
        s.rows.insert(
            key.clone(),
            BookStatus {
                session: session.clone(),
                revision: 0,
                phase: "synchronizing",
                reason: None,
                resyncs,
                bytes: 0,
                buffered_bytes: 0,
                seen: None,
            },
        );
        Ok(BookLease {
            registry: self.clone(),
            key,
            session,
        })
    }
    pub fn usable(&self, book: &OrderBookTick) -> bool {
        let Some(q) = &book.book_quality else {
            return false;
        };
        if !q.synchronized {
            return false;
        }
        let s = self.inner.lock().unwrap();
        s.rows
            .get(&book_key(book.exchange, book.market, &book.symbol))
            .is_some_and(|r| {
                q.reconstructed_at.elapsed().as_millis() <= s.config.max_age_ms as u128
                    && r.session == q.session
                    && r.revision >= q.revision
                    && r.phase == "synchronized"
                    && r.seen
                        .is_some_and(|at| at.elapsed().as_millis() <= s.config.max_age_ms as u128)
            })
    }
    pub fn usable_revision(
        &self,
        exchange: &str,
        market: MarketKind,
        symbol: &str,
        session: &str,
        revision: u64,
    ) -> bool {
        let s = self.inner.lock().unwrap();
        s.rows
            .get(&book_key(exchange, market, symbol))
            .is_some_and(|r| {
                r.session == session
                    && r.revision >= revision
                    && r.phase == "synchronized"
                    && r.seen
                        .is_some_and(|at| at.elapsed().as_millis() <= s.config.max_age_ms as u128)
            })
    }
    pub async fn request_weight(&self, weight: u64) {
        loop {
            let at = {
                let mut s = self.inner.lock().unwrap();
                let at = s.next_request.max(s.blocked_until).max(Instant::now());
                s.next_request = at
                    + std::time::Duration::from_millis(
                        weight * 60000 / s.config.binance_weight_per_minute,
                    );
                at
            };
            tokio::time::sleep_until(at.into()).await;
            if Instant::now() >= self.inner.lock().unwrap().blocked_until {
                break;
            }
        }
    }
    pub fn throttle(&self, delay: std::time::Duration) {
        let mut s = self.inner.lock().unwrap();
        s.blocked_until = s.blocked_until.max(Instant::now() + delay);
    }
    pub fn report(&self) -> serde_json::Value {
        let s = self.inner.lock().unwrap();
        let rows=s.rows.iter().map(|(key,r)|{let fresh=r.seen.is_some_and(|at|at.elapsed().as_millis()<=s.config.max_age_ms as u128);serde_json::json!({"stream":key,"session":r.session,"revision":r.revision,"phase":if r.phase=="synchronized"&&!fresh {"stale"}else{r.phase},"reason":r.reason,"resyncs":r.resyncs,"ready":r.phase=="synchronized"&&fresh,"receipt_age_ms":r.seen.map(|at|at.elapsed().as_millis() as u64)})}).collect::<Vec<_>>();
        serde_json::json!({"model_version":"l2-reconstruction/v1","ready":rows.iter().all(|r|r["ready"]==true),"books":rows,"limits":s.config,"accounted_state_bytes":s.bytes,"buffered_bytes":s.buffered_bytes,"note":"bounded subscribed depth; Bybit transport ordering has no native gap attestation"})
    }
}
impl BookLease {
    pub fn synchronized(&self, book: &OrderBookTick, bytes: usize) -> Result<()> {
        let mut s = self.registry.inner.lock().unwrap();
        let old = s
            .rows
            .get(&self.key)
            .filter(|r| r.session == self.session)
            .context("book lease replaced")?
            .bytes;
        ensure!(
            s.bytes - old + bytes <= s.config.max_state_bytes,
            "global reconstructed book memory budget exceeded"
        );
        s.bytes = s.bytes - old + bytes;
        let r = s.rows.get_mut(&self.key).unwrap();
        r.phase = "synchronized";
        r.reason = None;
        r.seen = Some(Instant::now());
        r.revision = book.book_quality.as_ref().map_or(0, |q| q.revision);
        r.bytes = bytes;
        Ok(())
    }
    pub fn buffered(&self, bytes: usize) -> Result<()> {
        let mut s = self.registry.inner.lock().unwrap();
        let old = s
            .rows
            .get(&self.key)
            .filter(|r| r.session == self.session)
            .context("book lease replaced")?
            .buffered_bytes;
        ensure!(
            s.buffered_bytes - old + bytes <= s.config.max_total_buffered_bytes,
            "global book bootstrap buffer budget exceeded"
        );
        s.buffered_bytes = s.buffered_bytes - old + bytes;
        s.rows.get_mut(&self.key).unwrap().buffered_bytes = bytes;
        Ok(())
    }
    pub fn is_synchronized(&self) -> bool {
        self.registry
            .inner
            .lock()
            .unwrap()
            .rows
            .get(&self.key)
            .is_some_and(|r| r.session == self.session && r.phase == "synchronized")
    }
    pub fn invalidate(&self, reason: &'static str) {
        let mut s = self.registry.inner.lock().unwrap();
        if let Some(r) = s.rows.get_mut(&self.key)
            && r.session == self.session
            && r.phase != "resync_required"
        {
            r.phase = "resync_required";
            r.reason = Some(reason);
        }
    }
}
impl Drop for BookLease {
    fn drop(&mut self) {
        self.invalidate("connection_ended_or_cancelled");
        let mut s = self.registry.inner.lock().unwrap();
        if let Some(r) = s.rows.get(&self.key)
            && r.session == self.session
        {
            let (bytes, buffered) = (r.bytes, r.buffered_bytes);
            s.bytes -= bytes;
            s.buffered_bytes -= buffered;
            let r = s.rows.get_mut(&self.key).unwrap();
            r.bytes = 0;
            r.buffered_bytes = 0;
        }
    }
}
pub fn book_key(exchange: &str, market: MarketKind, symbol: &str) -> String {
    format!(
        "{exchange}:{}:{}",
        if market == MarketKind::Spot {
            "spot"
        } else {
            "perp"
        },
        symbol.to_ascii_uppercase()
    )
}

/// Strict protocol codec shared by live connectors and raw replay tests.
pub fn levels(value: &serde_json::Value) -> Result<Vec<WireLevel>> {
    let rows = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("book side is not array"))?;
    ensure!(rows.len() <= 10000, "wire level budget exceeded");
    rows.iter()
        .map(|v| {
            let a = v
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("book row is not array"))?;
            ensure!(a.len() >= 2, "short book row");
            let price = a[0]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("price must preserve wire decimal"))?;
            let qty = a[1]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("qty must preserve wire decimal"))?;
            decimal(price)?;
            decimal(qty)?;
            Ok(WireLevel::new(price, qty))
        })
        .collect()
}
pub fn parse_update(
    protocol: Protocol,
    value: &serde_json::Value,
    snapshot: bool,
) -> Result<Update> {
    let number = |key: &str| -> Result<u64> {
        value[key]
            .as_u64()
            .or_else(|| value[key].as_str().and_then(|v| v.parse().ok()))
            .ok_or_else(|| anyhow::anyhow!("missing/invalid {key}"))
    };
    let (first, last, previous, ts, bids, asks) = match protocol {
        Protocol::BinanceSpot | Protocol::BinancePerp => {
            if snapshot {
                (
                    number("lastUpdateId")?,
                    number("lastUpdateId")?,
                    None,
                    value["E"].as_u64().unwrap_or_else(crate::types::now_ms),
                    "bids",
                    "asks",
                )
            } else {
                (
                    number("U")?,
                    number("u")?,
                    value["pu"].as_u64(),
                    number("E")?,
                    "b",
                    "a",
                )
            }
        }
        Protocol::Okx => {
            let last = number("seqId")?;
            let previous = value["prevSeqId"]
                .as_i64()
                .or_else(|| value["prevSeqId"].as_str().and_then(|v| v.parse().ok()))
                .ok_or_else(|| anyhow::anyhow!("missing OKX prevSeqId"))?;
            ensure!(previous >= -1, "invalid OKX prevSeqId");
            (
                last,
                last,
                if previous < 0 {
                    None
                } else {
                    Some(previous as u64)
                },
                number("ts")?,
                "bids",
                "asks",
            )
        }
        Protocol::Bybit => (
            number("u")?,
            number("u")?,
            Some(number("seq")?),
            number("ts")?,
            "b",
            "a",
        ),
    };
    let checksum = if let Some(c) = value.get("checksum") {
        Some(
            c.as_i64()
                .or_else(|| c.as_str().and_then(|v| v.parse().ok()))
                .filter(|v| i32::try_from(*v).is_ok())
                .ok_or_else(|| anyhow::anyhow!("invalid checksum"))? as i32,
        )
    } else {
        None
    };
    if protocol == Protocol::Bybit && !snapshot && last == 1 {
        bail!("Bybit restart requires snapshot");
    }
    Ok(Update {
        first,
        last,
        previous,
        ts_ms: ts,
        snapshot,
        bids: levels(&value[bids])?,
        asks: levels(&value[asks])?,
        checksum,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(last: u64, snapshot: bool) -> Update {
        Update {
            first: last,
            last,
            previous: None,
            ts_ms: 1000 + last,
            snapshot,
            bids: if snapshot {
                vec![WireLevel::new("99.00", "1"), WireLevel::new("98", "2")]
            } else {
                vec![]
            },
            asks: if snapshot {
                vec![WireLevel::new("101.0", "3"), WireLevel::new("102", "4")]
            } else {
                vec![]
            },
            checksum: None,
        }
    }
    fn book(p: Protocol) -> LocalBook {
        LocalBook::new(
            p,
            MarketKind::Spot,
            "BTCUSDT".into(),
            "fixture:1".into(),
            BookConfig::default(),
        )
    }
    #[test]
    fn spot_snapshot_bridge_delete_insert_gap_and_rebootstrap() {
        let mut b = book(Protocol::BinanceSpot);
        assert_eq!(b.apply(&frame(10, true)).unwrap(), Applied::Ignored);
        assert!(b.tick(None).is_err());
        assert_eq!(b.apply(&frame(9, false)).unwrap(), Applied::Ignored);
        let mut u = frame(12, false);
        u.first = 10;
        u.bids = vec![WireLevel::new("99.00", "0"), WireLevel::new("100", "5")];
        assert_eq!(b.apply(&u).unwrap(), Applied::Publish);
        let t = b.tick(None).unwrap();
        assert_eq!(t.bids[0].price, 100.0);
        assert_eq!(t.bids[0].qty, 5.0);
        assert!(b.apply(&frame(14, false)).is_err());
        assert!(b.tick(None).is_err());
        b.apply(&frame(20, true)).unwrap();
        b.apply(&frame(21, false)).unwrap();
        assert_eq!(
            b.tick(None)
                .unwrap()
                .book_quality
                .unwrap()
                .snapshot_generation,
            2
        );
    }
    #[test]
    fn perp_accepts_equal_boundary_then_requires_pu() {
        let mut b = book(Protocol::BinancePerp);
        b.apply(&frame(10, true)).unwrap();
        let mut first = frame(10, false);
        first.first = 8;
        first.previous = Some(7);
        assert_eq!(b.apply(&first).unwrap(), Applied::Publish);
        let mut next = frame(15, false);
        next.first = 11;
        next.previous = Some(10);
        b.apply(&next).unwrap();
        next.last = 16;
        next.first = 16;
        next.previous = Some(14);
        assert!(b.apply(&next).is_err());
        assert!(b.tick(None).is_err());
    }
    #[test]
    fn okx_signed_wire_crc_and_sequence_reset() {
        let mut b = book(Protocol::Okx);
        let mut snap = frame(100, true);
        // Independent fixed wire checksum, including trailing decimal zeros.
        let mut c = flate2::Crc::new();
        c.update(b"99.00:1:101.0:3:98:2:102:4");
        snap.checksum = Some(c.sum() as i32);
        b.apply(&snap).unwrap();
        assert_eq!(
            b.tick(snap.checksum)
                .unwrap()
                .book_quality
                .unwrap()
                .checksum_policy,
            "server_crc32_top25_verified"
        );
        let mut reset = frame(2, false);
        reset.previous = Some(100);
        reset.ts_ms = 2000;
        reset.checksum = snap.checksum;
        b.apply(&reset).unwrap();
        let mut noop = frame(2, false);
        noop.previous = Some(2);
        noop.ts_ms = 2001;
        noop.checksum = snap.checksum;
        b.apply(&noop).unwrap();
        let mut bad = frame(3, false);
        bad.previous = Some(2);
        bad.ts_ms = 2002;
        bad.bids = vec![WireLevel::new("99.00", "9")];
        bad.checksum = snap.checksum;
        assert!(b.apply(&bad).is_err());
        assert!(b.tick(None).is_err());
    }
    #[test]
    fn bad_delta_never_exposes_partial_state() {
        for changes in [
            vec![WireLevel::new("100", "1"), WireLevel::new("100.0", "2")],
            vec![WireLevel::new("100", "-1")],
            vec![WireLevel::new("1e2", "2")],
            vec![WireLevel::new("100", "NaN")],
            vec![WireLevel::new("102", "1")],
        ] {
            let mut b = book(Protocol::Okx);
            b.apply(&frame(1, true)).unwrap();
            let mut u = frame(2, false);
            u.previous = Some(1);
            u.bids = changes;
            assert!(b.apply(&u).is_err());
            assert!(b.tick(None).is_err());
        }
    }
    #[test]
    fn snapshot_fence_and_exact_float_aliases_fail_closed() {
        let mut b = book(Protocol::BinanceSpot);
        b.apply(&frame(1, true)).unwrap();
        let mut u = frame(2, false);
        u.bids = vec![
            WireLevel::new("99.00", "0"),
            WireLevel::new("98", "0"),
            WireLevel::new("97", "1"),
        ];
        assert!(b.apply(&u).is_err());
        let mut b = book(Protocol::Okx);
        let mut u = frame(1, true);
        u.bids = vec![
            WireLevel::new("99.000000000000000001", "1"),
            WireLevel::new("99.000000000000000002", "1"),
        ];
        b.apply(&u).unwrap();
        assert!(b.tick(None).is_err());
        let mut b = book(Protocol::Okx);
        u.bids = vec![WireLevel::new("99.000000000000000001", "1")];
        u.asks = vec![WireLevel::new("99.000000000000000002", "1")];
        b.apply(&u).unwrap();
        assert!(
            b.tick(None).is_err(),
            "opposite sides must remain distinct in public f64 output"
        );
    }
    #[test]
    fn bybit_snapshot_required_duplicate_conflict_and_restart() {
        let mut b = book(Protocol::Bybit);
        assert!(b.apply(&frame(3, false)).is_err());
        b.apply(&frame(3, true)).unwrap();
        let u = frame(7, false);
        b.apply(&u).unwrap();
        assert_eq!(b.apply(&u).unwrap(), Applied::Ignored);
        let mut conflict = u;
        conflict.bids = vec![WireLevel::new("99.00", "8")];
        assert!(b.apply(&conflict).is_err());
        let mut restart = frame(1, true);
        restart.ts_ms = 2000;
        b.apply(&restart).unwrap();
        assert_eq!(b.tick(None).unwrap().last_update_id, Some(1));
    }
    #[test]
    fn lease_drop_and_replaced_session_reject_old_cache() {
        let r = BookRegistry::default();
        let lease = r.begin("okx", MarketKind::Spot, "BTCUSDT").unwrap();
        let mut b = LocalBook::new(
            Protocol::Okx,
            MarketKind::Spot,
            "BTCUSDT".into(),
            lease.session.clone(),
            BookConfig::default(),
        );
        b.apply(&frame(1, true)).unwrap();
        let t = b.tick(None).unwrap();
        lease.synchronized(&t, b.owned_bytes()).unwrap();
        assert!(r.usable(&t));
        let mut stalled_cache = t.clone();
        stalled_cache
            .book_quality
            .as_mut()
            .unwrap()
            .reconstructed_at = Instant::now() - std::time::Duration::from_secs(6);
        lease.synchronized(&t, b.owned_bytes()).unwrap();
        assert!(
            !r.usable(&stalled_cache),
            "new arrivals cannot renew a stalled consumer's old cached view"
        );
        let newer = r.begin("okx", MarketKind::Spot, "BTCUSDT").unwrap();
        drop(lease);
        assert!(!r.usable(&t));
        assert_eq!(r.report()["books"][0]["session"], newer.session);
        drop(newer);
        assert_eq!(r.report()["ready"], false);
    }
    #[test]
    fn contract_quantities_cannot_be_used_without_verified_units() {
        let mut b = LocalBook::new(
            Protocol::Okx,
            MarketKind::Perp,
            "BTC-USDT-SWAP".into(),
            "units".into(),
            Default::default(),
        );
        b.apply(&frame(1, true)).unwrap();
        assert!(b.tick(None).is_err());
        b.set_contract_value(0.01).unwrap();
        let t = b.tick(None).unwrap();
        assert_eq!(t.bids[0].qty, 0.01);
        assert_eq!(t.book_quality.as_ref().unwrap().exact_bids[0][1], "1");
        assert_eq!(
            t.book_quality.as_ref().unwrap().native_quantity_unit,
            "contracts"
        );
    }
    #[tokio::test]
    async fn stale_book_and_global_state_budget_fail_closed() {
        let r = BookRegistry::default();
        r.configure(BookConfig {
            max_age_ms: 1,
            max_state_bytes: 65536,
            ..Default::default()
        });
        let lease = r.begin("okx", MarketKind::Spot, "BTC").unwrap();
        let mut b = LocalBook::new(
            Protocol::Okx,
            MarketKind::Spot,
            "BTC".into(),
            lease.session.clone(),
            Default::default(),
        );
        b.apply(&frame(1, true)).unwrap();
        let t = b.tick(None).unwrap();
        assert!(lease.synchronized(&t, 65537).is_err());
        assert!(!r.usable(&t));
        lease.synchronized(&t, 1000).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert!(!r.usable(&t));
        assert_eq!(r.report()["books"][0]["phase"], "stale");
    }
    #[test]
    fn global_budgets_and_codec_reject_invalid_contracts() {
        let r = BookRegistry::default();
        r.configure(BookConfig {
            max_symbols: 1,
            max_state_bytes: 65536,
            max_total_buffered_bytes: 65536,
            ..Default::default()
        });
        let lease = r.begin("okx", MarketKind::Spot, "A").unwrap();
        assert!(r.begin("okx", MarketKind::Spot, "B").is_err());
        assert!(lease.buffered(65537).is_err());
        assert!(
            parse_update(
                Protocol::Okx,
                &serde_json::json!({"seqId":1,"ts":"1","bids":[],"asks":[]}),
                true
            )
            .is_err()
        );
        assert!(levels(&serde_json::json!([[99, 1]])).is_err());
        assert!(decimal("9999999999999999999999999999999999999999").is_err());
    }
}
