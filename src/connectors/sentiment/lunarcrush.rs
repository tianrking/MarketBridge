use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde_json::Value;
use tracing::warn;

use crate::config::LunarCrushConfig;
use crate::connectors::aggregate::common::{
    emit_external_signal, parse_f64_value, require_api_key,
};
use crate::source::{ExchangeSource, SourceContext};

pub struct LunarCrushPoller {
    cfg: LunarCrushConfig,
    client: reqwest::Client,
}

impl LunarCrushPoller {
    pub fn new(cfg: LunarCrushConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for LunarCrushPoller {
    fn name(&self) -> &'static str {
        "lunarcrush"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match require_api_key(&self.cfg.api_key, &self.cfg.api_key_env) {
                Ok(api_key) => {
                    for symbol in &self.cfg.symbols {
                        match fetch_symbol(&self.client, &self.cfg.base_url, &api_key, symbol).await
                        {
                            Ok(raw) => {
                                emit_external_signal(
                                    &ctx,
                                    self.name(),
                                    "sentiment",
                                    Some(symbol),
                                    "lunarcrush_social_score",
                                    social_score(&raw),
                                    Some(raw),
                                )
                                .await?;
                            }
                            Err(error) => {
                                warn!(symbol=%symbol, %error, "lunarcrush refresh failed")
                            }
                        }
                    }
                }
                Err(error) => warn!(%error, "lunarcrush api key missing"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_symbol(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    symbol: &str,
) -> Result<Value> {
    let mut url = Url::parse(base_url)?.join("coins/list/v1")?;
    url.query_pairs_mut()
        .append_pair("symbol", symbol)
        .append_pair("key", api_key);
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to decode lunarcrush response")
}

fn social_score(raw: &Value) -> Option<f64> {
    for key in ["galaxy_score", "alt_rank", "social_score", "sentiment"] {
        if let Some(value) = raw
            .pointer(&format!("/data/0/{key}"))
            .and_then(parse_f64_value)
        {
            return Some(value);
        }
    }
    None
}
