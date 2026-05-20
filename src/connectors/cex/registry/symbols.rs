pub(super) fn split_quote(s: &str) -> (&str, &str) {
    for q in ["USDT", "USDC", "USD", "KRW", "JPY", "BTC", "ETH"] {
        if let Some(base) = s.strip_suffix(q) {
            return (base, q);
        }
    }
    if s.len() >= 6 {
        let (b, q) = s.split_at(s.len() - 4);
        return (b, q);
    }
    (s, "USDT")
}

pub(super) fn to_binance(s: &str) -> String {
    s.to_string()
}

pub(super) fn to_okx(s: &str) -> String {
    to_dash(s)
}

pub(super) fn to_okx_swap(s: &str) -> String {
    format!("{}-SWAP", to_dash(s))
}

pub(super) fn to_dash(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("{b}-{q}")
}

pub(super) fn to_slash(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("{b}/{q}")
}

pub(super) fn to_underscore(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("{b}_{q}")
}

pub(super) fn to_bitfinex(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("t{b}{q}")
}

pub(super) fn to_kucoin_perp(s: &str) -> String {
    format!("{s}M")
}

pub(super) fn to_htx_perp(s: &str) -> String {
    to_dash(s)
}

pub(super) fn to_bitfinex_perp(s: &str) -> String {
    let (b, q) = split_quote(s);
    format!("t{b}F0:{q}F0")
}

pub(super) fn to_kraken_perp(s: &str) -> String {
    // Kraken futures symbols vary across venues; pass through for user override compatibility.
    s.to_string()
}

pub(super) fn to_hyperliquid_coin(s: &str) -> String {
    split_quote(s).0.to_string()
}

pub(super) fn to_dydx_market(s: &str) -> String {
    let (base, quote) = split_quote(s);
    format!("{base}-{quote}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_converters_work_for_usdt_pairs() {
        assert_eq!(to_okx("BTCUSDT"), "BTC-USDT");
        assert_eq!(to_okx_swap("ETHUSDT"), "ETH-USDT-SWAP");
        assert_eq!(to_underscore("BTCUSDT"), "BTC_USDT");
        assert_eq!(to_slash("ETHUSDT"), "ETH/USDT");
        assert_eq!(to_bitfinex("BTCUSDT"), "tBTCUSDT");
        assert_eq!(to_kucoin_perp("BTCUSDT"), "BTCUSDTM");
        assert_eq!(to_htx_perp("BTCUSDT"), "BTC-USDT");
        assert_eq!(to_bitfinex_perp("BTCUSDT"), "tBTCF0:USDTF0");
        assert_eq!(to_hyperliquid_coin("BTCUSDT"), "BTC");
        assert_eq!(to_dydx_market("BTCUSDT"), "BTC-USDT");
        assert_eq!(to_underscore("BTCJPY"), "BTC_JPY");
    }
}
