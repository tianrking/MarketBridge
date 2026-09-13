use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::DateTime;
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
    published_at: Option<String>,
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
                        let signal = external_signal_for_post(
                            post,
                            &self.cfg.currencies,
                            crate::types::now_ms(),
                        );
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

fn external_signal_for_post(
    post: CryptoPanicPost,
    currencies: &[String],
    ts_ms: u64,
) -> crate::types::ExternalSignalTick {
    let score = post.votes.as_ref().map(|votes| {
        votes.positive.unwrap_or(0.0) - votes.negative.unwrap_or(0.0)
            + votes.important.unwrap_or(0.0)
    });
    crate::types::ExternalSignalTick {
        source: "cryptopanic",
        // `news_item` repeats for every post. Keep the canonical URL in the
        // snapshot key so the bounded feed does not overwrite itself.
        source_instance: Some(post.url.clone().into_boxed_str()),
        source_time_ms: post.published_at.as_deref().and_then(parse_rfc3339_ms),
        category: "news".into(),
        symbol: Some(currencies.join(",").into_boxed_str()),
        metric: "news_item".into(),
        value: score,
        score,
        title: Some(post.title.into_boxed_str()),
        url: Some(post.url.into_boxed_str()),
        ts_ms,
        raw: None,
    }
}

fn parse_rfc3339_ms(value: &str) -> Option<u64> {
    u64::try_from(DateTime::parse_from_rfc3339(value).ok()?.timestamp_millis()).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_news_url_gets_a_distinct_snapshot_instance() {
        let first = external_signal_for_post(
            CryptoPanicPost {
                title: "first".into(),
                url: "https://news.example/first".into(),
                published_at: Some("2026-09-14T00:00:00Z".into()),
                votes: None,
            },
            &["BTC".into()],
            1,
        );
        let second = external_signal_for_post(
            CryptoPanicPost {
                title: "second".into(),
                url: "https://news.example/second".into(),
                published_at: None,
                votes: None,
            },
            &["BTC".into()],
            2,
        );
        assert_ne!(first.source_instance, second.source_instance);
        assert_eq!(first.metric, "news_item".into());
        assert_eq!(first.source_time_ms, Some(1_789_344_000_000));
        assert_eq!(second.source_time_ms, None);
    }
}
