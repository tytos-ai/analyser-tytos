use chrono::{DateTime, Duration, Utc};
use std::fmt;

/// Error type for time parsing operations
#[derive(Debug)]
pub enum ParseTimeError {
    InvalidFormat(String),
    InvalidNumber(String),
    UnsupportedUnit(String),
    InvalidRange(String),
}

impl fmt::Display for ParseTimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseTimeError::InvalidFormat(s) => write!(f, "Invalid time format: {}", s),
            ParseTimeError::InvalidNumber(s) => write!(f, "Invalid number in time string: {}", s),
            ParseTimeError::UnsupportedUnit(s) => write!(f, "Unsupported time unit: {}", s),
            ParseTimeError::InvalidRange(s) => write!(f, "Invalid time range: {}", s),
        }
    }
}

impl std::error::Error for ParseTimeError {}

/// Parse relative time strings like "1h", "7d", "1m" into Duration
pub fn parse_relative_time(input: &str) -> Result<Duration, ParseTimeError> {
    let input = input.trim().to_lowercase();

    if input.is_empty() {
        return Err(ParseTimeError::InvalidFormat("Empty time string".to_string()));
    }

    // Split number and unit
    let (number_part, unit_part) = split_number_unit(&input)?;

    // Parse the number
    let number: i64 = number_part.parse()
        .map_err(|_| ParseTimeError::InvalidNumber(number_part.to_string()))?;

    if number <= 0 {
        return Err(ParseTimeError::InvalidNumber("Time amount must be positive".to_string()));
    }

    // Convert to duration based on unit
    let duration = match unit_part {
        "h" => Duration::hours(number),
        "d" => Duration::days(number),
        "w" => Duration::weeks(number),
        "m" => Duration::days(number * 30), // Approximate month as 30 days
        "y" => Duration::days(number * 365), // Approximate year as 365 days
        _ => return Err(ParseTimeError::UnsupportedUnit(unit_part.to_string())),
    };

    // Validate reasonable ranges
    let max_duration = Duration::days(365 * 2); // 2 years maximum
    if duration > max_duration {
        return Err(ParseTimeError::InvalidRange(
            format!("Time range too large: {} (max: 2 years)", input)
        ));
    }

    Ok(duration)
}

/// Split a time string like "7d" into ("7", "d")
fn split_number_unit(input: &str) -> Result<(&str, &str), ParseTimeError> {
    let mut number_end = 0;

    for (i, ch) in input.char_indices() {
        if ch.is_ascii_digit() {
            number_end = i + 1;
        } else {
            break;
        }
    }

    if number_end == 0 {
        return Err(ParseTimeError::InvalidFormat(
            "No number found at start of time string".to_string()
        ));
    }

    if number_end == input.len() {
        return Err(ParseTimeError::InvalidFormat(
            "No time unit found after number".to_string()
        ));
    }

    let number_part = &input[..number_end];
    let unit_part = &input[number_end..];

    Ok((number_part, unit_part))
}

/// Convert DateTime<Utc> to Zerion timestamp format (13-digit milliseconds)
pub fn to_zerion_timestamp(datetime: DateTime<Utc>) -> String {
    datetime.timestamp_millis().to_string()
}

/// Calculate time range timestamps for Zerion API
/// Returns (min_mined_at, max_mined_at) where max_mined_at is current time
pub fn calculate_time_range(relative_time: &str) -> Result<(String, String), ParseTimeError> {
    let duration = parse_relative_time(relative_time)?;
    let now = Utc::now();
    let start_time = now - duration;

    let min_mined_at = to_zerion_timestamp(start_time);
    let max_mined_at = to_zerion_timestamp(now);

    Ok((min_mined_at, max_mined_at))
}

/// Validate if a time range string is supported
pub fn is_valid_time_range(input: &str) -> bool {
    parse_relative_time(input).is_ok()
}

/// Get a list of example supported time ranges
pub fn supported_time_ranges() -> Vec<&'static str> {
    vec![
        "1h", "2h", "6h", "12h", "24h",
        "1d", "2d", "7d", "14d", "30d",
        "1w", "2w", "4w",
        "1m", "3m", "6m", "12m",
        "1y"
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hours() {
        let duration = parse_relative_time("1h").unwrap();
        assert_eq!(duration, Duration::hours(1));

        let duration = parse_relative_time("24h").unwrap();
        assert_eq!(duration, Duration::hours(24));
    }

    #[test]
    fn test_parse_days() {
        let duration = parse_relative_time("7d").unwrap();
        assert_eq!(duration, Duration::days(7));

        let duration = parse_relative_time("30d").unwrap();
        assert_eq!(duration, Duration::days(30));
    }

    #[test]
    fn test_parse_weeks() {
        let duration = parse_relative_time("1w").unwrap();
        assert_eq!(duration, Duration::weeks(1));

        let duration = parse_relative_time("4w").unwrap();
        assert_eq!(duration, Duration::weeks(4));
    }

    #[test]
    fn test_parse_months() {
        let duration = parse_relative_time("1m").unwrap();
        assert_eq!(duration, Duration::days(30));

        let duration = parse_relative_time("6m").unwrap();
        assert_eq!(duration, Duration::days(180));
    }

    #[test]
    fn test_parse_years() {
        let duration = parse_relative_time("1y").unwrap();
        assert_eq!(duration, Duration::days(365));
    }

    #[test]
    fn test_invalid_formats() {
        assert!(parse_relative_time("").is_err());
        assert!(parse_relative_time("abc").is_err());
        assert!(parse_relative_time("1").is_err());
        assert!(parse_relative_time("h").is_err());
        assert!(parse_relative_time("1x").is_err());
        assert!(parse_relative_time("0h").is_err());
        assert!(parse_relative_time("-1h").is_err());
    }

    #[test]
    fn test_case_insensitive() {
        let duration1 = parse_relative_time("1H").unwrap();
        let duration2 = parse_relative_time("1h").unwrap();
        assert_eq!(duration1, duration2);
    }

    #[test]
    fn test_calculate_time_range() {
        let (min_ts, max_ts) = calculate_time_range("1h").unwrap();

        // Both should be 13-digit strings
        assert_eq!(min_ts.len(), 13);
        assert_eq!(max_ts.len(), 13);

        // min should be less than max
        let min_value: i64 = min_ts.parse().unwrap();
        let max_value: i64 = max_ts.parse().unwrap();
        assert!(min_value < max_value);

        // Difference should be approximately 1 hour in milliseconds
        let diff = max_value - min_value;
        let one_hour_ms = 60 * 60 * 1000;
        assert!((diff - one_hour_ms).abs() < 1000); // Allow 1 second tolerance
    }

    #[test]
    fn test_is_valid_time_range() {
        assert!(is_valid_time_range("1h"));
        assert!(is_valid_time_range("7d"));
        assert!(is_valid_time_range("1m"));
        assert!(!is_valid_time_range("1x"));
        assert!(!is_valid_time_range(""));
        assert!(!is_valid_time_range("abc"));
    }
}