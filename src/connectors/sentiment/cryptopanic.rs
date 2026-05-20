use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::warn;

use crate::config::CryptoPanicConfig;
use crate::connectors::aggregate::common::configured_api_key;
use crate::source::{ExchangeSource, SourceContext};

pub struct CryptoPanicPoller {
    cfg: CryptoPanicConfig,
    client: reqwest::Client,
}

impl CryptoPanicPoller {
    pub fn new(cfg: CryptoPanicConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CryptoPanicResponse {
    results: Vec<CryptoPanicPost>,
}

#[derive(Debug, Deserialize)]
struct CryptoPanicPost {
    title: String,
    url: String,
    #[serde(default)]
    votes: Option<CryptoPanicVotes>,
}

#[derive(Debug, Deserialize)]
struct CryptoPanicVotes {
    #[serde(default)]
    positive: Option<f64>,
    #[serde(default)]
    negative: Option<f64>,
    #[serde(default)]
    important: Option<f64>,
}

#[async_trait]
impl ExchangeSource for CryptoPanicPoller {
    fn name(&self) -> &'static str {
        "cryptopanic"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_posts(&self.client, &self.cfg).await {
                Ok(posts) => {
                    for post in posts.into_iter().take(10) {
                        let score = post.votes.as_ref().map(|votes| {
                            votes.positive.unwrap_or(0.0) - votes.negative.unwrap_or(0.0)
                                + votes.important.unwrap_or(0.0)
                        });
                        let mut signal = crate::types::ExternalSignalTick {
                            source: self.name(),
                            category: "news".into(),
                            symbol: None,
                            metric: "news_item".into(),
                            value: score,
                            score,
                            title: Some(post.title.into_boxed_str()),
                            url: Some(post.url.into_boxed_str()),
                            ts_ms: crate::types::now_ms(),
                            raw: None,
                        };
                        signal.symbol = Some(self.cfg.currencies.join(",").into_boxed_str());
                        ctx.emit(crate::types::DataEvent::ExternalSignal(signal))
                            .await?;
                    }
                }
                Err(error) => warn!(%error, "cryptopanic refresh failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(1))).await;
        }
    }
}

async fn fetch_posts(
    client: &reqwest::Client,
    cfg: &CryptoPanicConfig,
) -> Result<Vec<CryptoPanicPost>> {
    let api_key = configured_api_key(&cfg.api_key, &cfg.api_key_env)
        .context("missing API key: CRYPTOPANIC_API_KEY")?;
    let mut url = Url::parse(&cfg.base_url)?.join("posts/")?;
    url.query_pairs_mut()
        .append_pair("auth_token", &api_key)
        .append_pair("kind", "news")
        .append_pair("currencies", &cfg.currencies.join(","));

    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<CryptoPanicResponse>()
        .await
        .context("failed to decode cryptopanic posts")?
        .results)
}
