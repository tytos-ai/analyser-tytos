use crate::{PnLError, Result};
use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use std::str::FromStr;
use tracing::debug;

/// Parse a general timeframe string to get the cutoff timestamp
/// Matches TypeScript logic exactly: /^(\d+)(s|min|h|d|m|y)$/
pub fn parse_general_timeframe(timeframe: &str) -> Result<DateTime<Utc>> {
    let re = Regex::new(r"^(\d+)(s|min|h|d|m|y)$")
        .map_err(|e| PnLError::TimeframeParse(format!("Regex error: {}", e)))?;
    
    let captures = re.captures(timeframe)
        .ok_or_else(|| {
            debug!("No valid match found for GENERAL_TIMEFRAME; returning null");
            PnLError::TimeframeParse(format!("Invalid timeframe format: {}", timeframe))
        })?;
    
    let amount: i64 = captures[1].parse()
        .map_err(|e| PnLError::TimeframeParse(format!("Invalid number: {}", e)))?;
    
    let unit = &captures[2];
    
    let now = Utc::now();
    let now_ms = now.timestamp_millis();
    
    // Match TypeScript offset calculations exactly
    let offset_ms = match unit {
        "s" => amount * 1000,
        "min" => amount * 60000,
        "h" => amount * 3600000,
        "d" => amount * 86400000,
        "m" => amount * 2592000000, // ~30 days (TypeScript uses this exact value)
        "y" => amount * 31536000000, // ~365 days (TypeScript uses this exact value)
        _ => return Err(PnLError::TimeframeParse(format!("Unknown time unit: {}", unit))),
    };
    
    let cutoff_ms = now_ms - offset_ms;
    let cutoff_dt = DateTime::from_timestamp_millis(cutoff_ms)
        .ok_or_else(|| PnLError::TimeframeParse("Invalid timestamp calculation".to_string()))?;
    
    debug!(
        "General cutoff => {} => {} (UTC seconds since epoch)",
        timeframe, cutoff_dt.timestamp()
    );
    
    Ok(cutoff_dt)
}

/// Parse a specific timeframe string (e.g., "2024-01-15", "2024-01-15T10:30:00Z")
/// Matches TypeScript: new Date(SPECIFIC_TIMEFRAME) logic
pub fn parse_specific_timeframe(timeframe: &str) -> Result<DateTime<Utc>> {
    // Try parsing as full ISO 8601 first (matches JavaScript Date parsing)
    if let Ok(dt) = DateTime::parse_from_rfc3339(timeframe) {
        let cutoff_dt = dt.with_timezone(&Utc);
        debug!(
            "Specific cutoff calculated => {} (UTC seconds since epoch)",
            cutoff_dt.timestamp()
        );
        return Ok(cutoff_dt);
    }
    
    // Try parsing as date only (JavaScript Date handles this)
    if let Ok(naive_date) = chrono::NaiveDate::from_str(timeframe) {
        let naive_datetime = naive_date.and_hms_opt(0, 0, 0)
            .ok_or_else(|| PnLError::TimeframeParse("Invalid time components".to_string()))?;
        let cutoff_dt = Utc.from_utc_datetime(&naive_datetime);
        debug!(
            "Specific cutoff calculated => {} (UTC seconds since epoch)",
            cutoff_dt.timestamp()
        );
        return Ok(cutoff_dt);
    }
    
    // Try other common formats that JavaScript Date.parse would handle
    // Add more flexible parsing similar to JS Date constructor
    
    Err(PnLError::TimeframeParse(format!("Invalid specific timeframe format: {}", timeframe)))
}

/// Check if a block time is within the specified timeframe
/// Matches TypeScript: isWithinTimeframe(blockTime: number): boolean
pub fn is_within_timeframe(block_time: DateTime<Utc>, cutoff: Option<DateTime<Utc>>) -> bool {
    match cutoff {
        Some(cutoff_time) => {
            let within = block_time >= cutoff_time;
            debug!(
                "Checking blockTime={} >= cutoff={} => {}",
                block_time.timestamp(), cutoff_time.timestamp(), within
            );
            within
        }
        None => {
            debug!(
                "No cutoff, so blockTime {} is considered within timeframe.",
                block_time.timestamp()
            );
            true
        }
    }
}

/// Get timeframe cutoff based on configuration (replicates TypeScript getTimeframeCutoff())
pub fn get_timeframe_cutoff(
    timeframe_mode: &str,
    general_timeframe: Option<&str>,
    specific_timeframe: Option<&str>,
) -> Option<DateTime<Utc>> {
    debug!(
        "TIMEFRAME_MODE={}, GENERAL_TIMEFRAME={:?}, SPECIFIC_TIMEFRAME={:?}",
        timeframe_mode, general_timeframe, specific_timeframe
    );
    
    match timeframe_mode {
        "specific" => {
            if let Some(timeframe) = specific_timeframe {
                match parse_specific_timeframe(timeframe) {
                    Ok(cutoff) => Some(cutoff),
                    Err(_) => {
                        debug!("Failed to parse specific timeframe; returning null");
                        None
                    }
                }
            } else {
                debug!("TIMEFRAME_MODE set to specific but no SPECIFIC_TIMEFRAME provided");
                None
            }
        }
        "general" => {
            if let Some(timeframe) = general_timeframe {
                match parse_general_timeframe(timeframe) {
                    Ok(cutoff) => Some(cutoff),
                    Err(_) => {
                        debug!("Failed to parse general timeframe; returning null");
                        None
                    }
                }
            } else {
                debug!("TIMEFRAME_MODE set to general but no GENERAL_TIMEFRAME provided");
                None
            }
        }
        _ => {
            debug!("TIMEFRAME_MODE set to none or missing data => no cutoff");
            None
        }
    }
}