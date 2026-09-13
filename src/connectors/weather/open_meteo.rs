use anyhow::{Context, Result, bail};
use reqwest::Url;
use serde_json::Value;

const FORECAST_BASE_URL: &str = "https://api.open-meteo.com/v1/forecast";
const ARCHIVE_BASE_URL: &str = "https://archive-api.open-meteo.com/v1/archive";
const DEFAULT_DAILY: &str = "temperature_2m_max,temperature_2m_min,precipitation_sum,precipitation_probability_max,weather_code";
const DEFAULT_HOURLY: &str =
    "temperature_2m,precipitation_probability,precipitation,relative_humidity_2m,wind_speed_10m";

#[derive(Debug, Clone)]
pub struct OpenMeteoRequest {
    pub latitude: f64,
    pub longitude: f64,
    pub mode: String,
    pub timezone: String,
    pub forecast_days: Option<u8>,
    pub past_days: Option<u8>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub hourly: Option<String>,
    pub daily: Option<String>,
}

pub async fn fetch_open_meteo(
    client: &reqwest::Client,
    request: &OpenMeteoRequest,
) -> Result<Value> {
    validate_request(request)?;
    let base_url = match request.mode.as_str() {
        "forecast" => FORECAST_BASE_URL,
        "archive" => ARCHIVE_BASE_URL,
        other => bail!("unsupported Open-Meteo mode: {other}; use forecast or archive"),
    };
    let url = build_url(base_url, request)?;
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to decode Open-Meteo response")
}

fn build_url(base_url: &str, request: &OpenMeteoRequest) -> Result<Url> {
    let mut url = Url::parse(base_url)?;
    {
        let mut params = url.query_pairs_mut();
        params
            .append_pair("latitude", &request.latitude.to_string())
            .append_pair("longitude", &request.longitude.to_string())
            .append_pair("timezone", &request.timezone)
            .append_pair("daily", request.daily.as_deref().unwrap_or(DEFAULT_DAILY))
            .append_pair(
                "hourly",
                request.hourly.as_deref().unwrap_or(DEFAULT_HOURLY),
            );
        if request.mode == "forecast" {
            if let Some(days) = request.forecast_days {
                params.append_pair("forecast_days", &days.to_string());
            }
            if let Some(days) = request.past_days {
                params.append_pair("past_days", &days.to_string());
            }
        } else {
            params
                .append_pair(
                    "start_date",
                    request.start_date.as_deref().unwrap_or_default(),
                )
                .append_pair("end_date", request.end_date.as_deref().unwrap_or_default());
        }
    }
    Ok(url)
}

fn validate_request(request: &OpenMeteoRequest) -> Result<()> {
    if !(-90.0..=90.0).contains(&request.latitude) || !(-180.0..=180.0).contains(&request.longitude)
    {
        bail!("latitude/longitude are outside WGS84 bounds");
    }
    if request.timezone.trim().is_empty() {
        bail!("timezone must not be empty");
    }
    match request.mode.as_str() {
        "forecast" => {
            if request.start_date.is_some() || request.end_date.is_some() {
                bail!("forecast mode does not accept start_date/end_date");
            }
            if request
                .forecast_days
                .is_some_and(|days| !(1..=16).contains(&days))
            {
                bail!("forecast_days must be between 1 and 16");
            }
            if request.past_days.is_some_and(|days| days > 92) {
                bail!("past_days must be at most 92");
            }
        }
        "archive" => {
            if request.forecast_days.is_some() || request.past_days.is_some() {
                bail!("archive mode does not accept forecast_days/past_days");
            }
            let start = request
                .start_date
                .as_deref()
                .context("archive requires start_date")?;
            let end = request
                .end_date
                .as_deref()
                .context("archive requires end_date")?;
            if !is_iso_date(start) || !is_iso_date(end) {
                bail!("start_date/end_date must use YYYY-MM-DD");
            }
            if start > end {
                bail!("start_date must not be after end_date");
            }
        }
        other => bail!("unsupported Open-Meteo mode: {other}; use forecast or archive"),
    }
    Ok(())
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forecast() -> OpenMeteoRequest {
        OpenMeteoRequest {
            latitude: 52.52,
            longitude: 13.41,
            mode: "forecast".to_string(),
            timezone: "UTC".to_string(),
            forecast_days: Some(3),
            past_days: None,
            start_date: None,
            end_date: None,
            hourly: None,
            daily: None,
        }
    }

    #[test]
    fn forecast_url_contains_defaults_and_bounds() {
        let url = build_url(FORECAST_BASE_URL, &forecast()).expect("url");
        let query = url.query().unwrap_or_default();
        assert!(query.contains("forecast_days=3"));
        assert!(query.contains("daily=temperature_2m_max"));
        assert!(!query.contains("start_date"));
    }

    #[test]
    fn archive_requires_ordered_iso_dates() {
        let mut request = forecast();
        request.mode = "archive".to_string();
        request.forecast_days = None;
        request.start_date = Some("2025-01-02".to_string());
        request.end_date = Some("2025-01-01".to_string());
        assert!(validate_request(&request).is_err());
        request.end_date = Some("2025-01-03".to_string());
        assert!(validate_request(&request).is_ok());
    }

    #[test]
    fn rejects_invalid_coordinates_and_dates() {
        let mut request = forecast();
        request.latitude = 91.0;
        assert!(validate_request(&request).is_err());
        request.latitude = 52.52;
        request.mode = "archive".to_string();
        request.forecast_days = None;
        request.start_date = Some("yesterday".to_string());
        request.end_date = Some("2025-01-03".to_string());
        assert!(validate_request(&request).is_err());
    }
}
