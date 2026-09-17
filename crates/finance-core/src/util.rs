//! Small shared helpers for validation.

use chrono::NaiveDate;

use crate::error::{CoreError, Result};

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn today() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// Validate a `YYYY-MM-DD` date string and return it normalised.
pub fn parse_date(s: &str) -> Result<String> {
    let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("'{}' is not a valid date (YYYY-MM-DD).", s.trim())))?;
    check_date_range(d)?;
    Ok(d.format("%Y-%m-%d").to_string())
}

pub fn check_date_range(d: NaiveDate) -> Result<()> {
    let min = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap();
    let max = NaiveDate::from_ymd_opt(2200, 12, 31).unwrap();
    if d < min || d > max {
        return Err(CoreError::Invalid(format!("Date {d} is out of range.")));
    }
    Ok(())
}

/// Trim and length-check a required text field.
pub fn required_text(field: &str, value: &str, max: usize) -> Result<String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(CoreError::Invalid(format!("{field} is required.")));
    }
    if v.chars().count() > max {
        return Err(CoreError::Invalid(format!("{field} must be {max} characters or fewer.")));
    }
    Ok(v.to_string())
}

/// Trim an optional text field; empty becomes `None`.
pub fn optional_text(field: &str, value: Option<&str>, max: usize) -> Result<Option<String>> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) if v.chars().count() > max => {
            Err(CoreError::Invalid(format!("{field} must be {max} characters or fewer.")))
        }
        Some(v) => Ok(Some(v.to_string())),
    }
}

/// Normalise a payee for matching: lowercase, collapse whitespace, drop
/// trailing reference numbers such as "#1234" or long digit runs.
pub fn normalize_payee(p: &str) -> String {
    let lower = p.to_lowercase();
    let words: Vec<&str> = lower
        .split_whitespace()
        .filter(|w| {
            let digits = w.chars().filter(|c| c.is_ascii_digit()).count();
            !(w.starts_with('#') || (digits >= 4 && digits * 2 >= w.len()))
        })
        .collect();
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(parse_date(" 2026-09-01 ").unwrap(), "2026-09-01");
        assert!(parse_date("2026-13-01").is_err());
        assert!(parse_date("09/01/2026").is_err());
        assert!(parse_date("1800-01-01").is_err());
    }

    #[test]
    fn text() {
        assert_eq!(required_text("Name", "  Chase  ", 10).unwrap(), "Chase");
        assert!(required_text("Name", "   ", 10).is_err());
        assert!(required_text("Name", "12345678901", 10).is_err());
        assert_eq!(optional_text("Notes", Some("  "), 10).unwrap(), None);
    }

    #[test]
    fn payee_normalisation() {
        assert_eq!(normalize_payee("STARBUCKS  #1234 Seattle"), "starbucks seattle");
        assert_eq!(normalize_payee("AMAZON MKTPL 1A2B3C4D5E"), "amazon mktpl");
        assert_eq!(normalize_payee("Rent"), "rent");
    }
}
