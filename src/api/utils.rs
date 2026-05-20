use std::collections::HashSet;

pub fn parse_csv_set_upper(s: String) -> HashSet<String> {
    s.split(',')
        .map(|x| x.trim().to_ascii_uppercase())
        .filter(|x| !x.is_empty())
        .collect()
}

pub fn parse_csv_set_lower(s: String) -> HashSet<String> {
    s.split(',')
        .map(|x| x.trim().to_ascii_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}

pub fn parse_csv_vec(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_csv_set_lower, parse_csv_set_upper, parse_csv_vec};

    #[test]
    fn csv_helpers_trim_drop_empty_and_normalize_case() {
        assert!(parse_csv_set_upper(" btcusdt, ETHUSDT,, ".to_string()).contains("BTCUSDT"));
        assert!(parse_csv_set_lower(" OKX, ByBit,, ".to_string()).contains("bybit"));
        assert_eq!(
            parse_csv_vec("a, b,,c"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }
}
