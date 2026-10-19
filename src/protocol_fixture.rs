//! Explicit loopback-only acceptance inputs exercising production protocol code.
use crate::{
    source::{ExchangeSource, SourceContext},
    types::MarketKind,
};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub symbol: String,
    pub book_ws: String,
    pub depth_rest: String,
    pub trade_ws: String,
    pub agg_rest: String,
}
impl Config {
    pub fn validate(&self, api: std::net::SocketAddr) -> Result<()> {
        ensure!(
            api.ip().is_loopback(),
            "protocol fixture requires loopback API bind"
        );
        ensure!(
            !self.symbol.is_empty()
                && self.symbol.len() <= 32
                && self
                    .symbol
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "invalid fixture symbol"
        );
        for (value, scheme) in [
            (&self.book_ws, "ws"),
            (&self.trade_ws, "ws"),
            (&self.depth_rest, "http"),
            (&self.agg_rest, "http"),
        ] {
            let url = reqwest::Url::parse(value)?;
            let local = match url.host() {
                Some(host) => host
                    .to_string()
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback()),
                _ => false,
            };
            ensure!(
                local
                    && url.scheme() == scheme
                    && url.port().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none(),
                "fixture URL must be an explicit loopback endpoint without credentials"
            );
        }
        Ok(())
    }
}

pub struct Source {
    pub config: Config,
    pub depth: bool,
}
#[async_trait]
impl ExchangeSource for Source {
    fn name(&self) -> &'static str {
        "binance"
    }
    fn source_type(&self) -> &'static str {
        if self.depth { "depth" } else { "trade" }
    }
    fn label(&self) -> String {
        format!("acceptance_fixture/binance-{}", self.source_type())
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.depth {
            crate::connectors::cex::reconstructed_depth::run_fixture_once(
                &self.config.symbol,
                ctx,
                self.config.book_ws.clone(),
                self.config.depth_rest.clone(),
            )
            .await
        } else {
            crate::connectors::cex::binance::run_binance_trades_endpoint(
                MarketKind::Spot,
                std::slice::from_ref(&self.config.symbol),
                ctx,
                &self.config.trade_ws,
            )
            .await
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_rejects_public_endpoints_credentials_and_public_api() {
        let config = Config {
            symbol: "BTCUSDT".into(),
            book_ws: "ws://127.0.0.1:1234/book".into(),
            trade_ws: "ws://[::1]:1234/trades".into(),
            depth_rest: "http://127.0.0.1:1234/depth".into(),
            agg_rest: "http://127.0.0.1:1234/agg".into(),
        };
        let api = "127.0.0.1:8080".parse().unwrap();
        config.validate(api).unwrap();
        assert!(config.validate("0.0.0.0:8080".parse().unwrap()).is_err());
        for value in [
            "https://api.binance.com/agg",
            "http://localhost:1234/a",
            "http://u:p@127.0.0.1:1234/a",
            "http://127.0.0.1/a",
            "http://127.0.0.1:1234/a#x",
        ] {
            let mut bad = config.clone();
            bad.agg_rest = value.into();
            assert!(bad.validate(api).is_err());
        }
    }
}
