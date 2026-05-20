use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OptionSummary {
    pub venue: String,
    pub currency: String,
    pub instrument_name: String,
    pub option_type: Option<String>,
    pub strike: Option<f64>,
    pub expiry_time: Option<String>,
    pub bid_price: Option<f64>,
    pub ask_price: Option<f64>,
    pub mark_price: Option<f64>,
    pub mark_iv: Option<f64>,
    pub underlying_price: Option<f64>,
    pub underlying_index: Option<String>,
    pub open_interest: Option<f64>,
}

pub fn option_side_from_code(code: &str) -> String {
    match code {
        "C" | "CALL" => "call",
        "P" | "PUT" => "put",
        other => other,
    }
    .to_string()
}

pub fn parse_yy_mm_dd_expiry(text: &str) -> Option<String> {
    if text.len() != 6 || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year = 2000 + text[0..2].parse::<i32>().ok()?;
    let month = text[2..4].parse::<u32>().ok()?;
    let day = text[4..6].parse::<u32>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}T08:00:00Z"))
}

pub fn parse_day_month_year_expiry(text: &str) -> Option<String> {
    if text.len() < 7 {
        return None;
    }
    let day = text[0..2].parse::<u32>().ok()?;
    let month = month_number(&text[2..5])?;
    let year = 2000 + text[5..7].parse::<i32>().ok()?;
    Some(format!("{year:04}-{month:02}-{day:02}T08:00:00Z"))
}

pub fn parse_f64_opt(text: Option<&str>) -> Option<f64> {
    text.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        value.parse::<f64>().ok()
    })
}

fn month_number(text: &str) -> Option<u32> {
    match &text.to_ascii_uppercase()[..] {
        "JAN" => Some(1),
        "FEB" => Some(2),
        "MAR" => Some(3),
        "APR" => Some(4),
        "MAY" => Some(5),
        "JUN" => Some(6),
        "JUL" => Some(7),
        "AUG" => Some(8),
        "SEP" => Some(9),
        "OCT" => Some(10),
        "NOV" => Some(11),
        "DEC" => Some(12),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_option_expiry_formats() {
        assert_eq!(
            parse_yy_mm_dd_expiry("260626"),
            Some("2026-06-26T08:00:00Z".to_string())
        );
        assert_eq!(
            parse_day_month_year_expiry("26MAR27"),
            Some("2027-03-26T08:00:00Z".to_string())
        );
    }
}
