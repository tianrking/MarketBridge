use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone, Utc};
use tracing::warn;

use crate::config::FarsideEtfConfig;
use crate::connectors::aggregate::common::emit_external_signal_at;
use crate::source::{ExchangeSource, SourceContext};

pub struct FarsideEtfPoller {
    cfg: FarsideEtfConfig,
    client: reqwest::Client,
}

#[derive(Debug, Clone, PartialEq)]
struct EtfFlowRow {
    date: NaiveDate,
    flow_musd: f64,
}

impl FarsideEtfPoller {
    pub fn new(cfg: FarsideEtfConfig) -> Self {
        Self {
            cfg,
            client: reqwest::Client::builder()
                .user_agent("MarketBridge/research-only")
                .build()
                .expect("static HTTP client settings"),
        }
    }
}

#[async_trait]
impl ExchangeSource for FarsideEtfPoller {
    fn name(&self) -> &'static str {
        "farside_etf"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        loop {
            match fetch_latest(&self.client, &self.cfg.url).await {
                Ok(row) => {
                    let source_time_ms = Utc
                        .from_utc_datetime(&row.date.and_hms_opt(0, 0, 0).expect("valid midnight"))
                        .timestamp_millis() as u64;
                    emit_external_signal_at(
                        &ctx,
                        self.name(),
                        "macro_flow",
                        Some(&self.cfg.asset),
                        "net_flow_usd_millions",
                        Some(row.flow_musd),
                        Some(source_time_ms),
                        Some(serde_json::json!({
                            "date": row.date.to_string(),
                            "flow_musd": row.flow_musd,
                            "source_url": self.cfg.url,
                            "unit": "USD millions",
                            "research_only": true,
                        })),
                    )
                    .await?;
                }
                Err(error) => warn!(%error, "farside ETF flow refresh failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.poll_secs.max(60))).await;
        }
    }
}

async fn fetch_latest(client: &reqwest::Client, url: &str) -> Result<EtfFlowRow> {
    let html = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
        .context("failed to read Farside ETF flow page")?;
    parse_latest(&html).ok_or_else(|| anyhow!("Farside ETF flow table has no dated total row"))
}

fn parse_latest(html: &str) -> Option<EtfFlowRow> {
    let mut rows = html
        .split("<tr")
        .skip(1)
        .filter_map(parse_row)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.date);
    rows.pop()
}

fn parse_row(fragment: &str) -> Option<EtfFlowRow> {
    let cells = fragment
        .split("<td")
        .skip(1)
        .filter_map(cell_text)
        .collect::<Vec<_>>();
    let date = cells.first().and_then(|value| parse_date(value))?;
    let flow_musd = cells.iter().rev().find_map(|value| parse_flow(value))?;
    Some(EtfFlowRow { date, flow_musd })
}

fn cell_text(fragment: &str) -> Option<String> {
    let value = fragment.split_once('>')?.1.split_once("</td>")?.0;
    let mut text = String::new();
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => text.push(character),
            _ => {}
        }
    }
    let text = text.replace("&nbsp;", " ").replace("&#160;", " ");
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%d %b %Y")
        .or_else(|_| NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d"))
        .ok()
}

fn parse_flow(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed == "—" {
        return None;
    }
    let negative = trimmed.starts_with('(') && trimmed.ends_with(')');
    let cleaned = trimmed.trim_matches(['(', ')']).replace([',', '$'], "");
    let number = cleaned.parse::<f64>().ok()?;
    Some(if negative { -number } else { number })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_latest_total_row_and_nested_cells() {
        let html = r#"
          <table><tr><td>01 Sep 2026</td><td>1.0</td><td>(20.5)</td></tr>
          <tr><td>02 Sep 2026</td><td><span>3.0</span></td><td>125.7</td></tr></table>
        "#;
        assert_eq!(
            parse_latest(html),
            Some(EtfFlowRow {
                date: NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(),
                flow_musd: 125.7,
            })
        );
    }

    #[test]
    fn parses_parenthesized_and_comma_flow() {
        assert_eq!(parse_flow("(1,234.5)"), Some(-1234.5));
        assert_eq!(parse_flow("—"), None);
    }
}
