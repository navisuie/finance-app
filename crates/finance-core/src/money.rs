//! Money is stored as integer cents (i64). Never use floating point for
//! balances.

use crate::error::{CoreError, Result};

/// Parse a user- or CSV-supplied amount such as `1,234.56`, `-$12.50`,
/// `(45.00)` (accounting negative) or `12`.
pub fn parse_cents(input: &str) -> Result<i64> {
    let bad = || CoreError::Invalid(format!("'{}' is not a valid amount", input.trim()));
    let mut s = input.trim().to_string();
    let mut negative = false;
    if s.starts_with('(') && s.ends_with(')') {
        negative = true;
        s = s[1..s.len() - 1].to_string();
    }
    s.retain(|c| c != ',' && c != '$' && !c.is_whitespace());
    if let Some(rest) = s.strip_prefix('-') {
        negative = !negative;
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix('+') {
        s = rest.to_string();
    }
    if s.is_empty() {
        return Err(bad());
    }
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s.as_str(), ""),
    };
    if frac.len() > 2
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || (whole.is_empty() && frac.is_empty())
    {
        return Err(bad());
    }
    let whole: i64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| bad())? };
    let frac: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().map_err(|_| bad())? * 10,
        _ => frac.parse().map_err(|_| bad())?,
    };
    let cents = whole.checked_mul(100).and_then(|w| w.checked_add(frac)).ok_or_else(bad)?;
    Ok(if negative { -cents } else { cents })
}

/// Format cents as `-$1,234.56`.
pub fn format_cents(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    let whole = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${}.{:02}", if neg { "-" } else { "" }, grouped, abs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_formats() {
        assert_eq!(parse_cents("1,234.56").unwrap(), 123456);
        assert_eq!(parse_cents("-$12.5").unwrap(), -1250);
        assert_eq!(parse_cents("(45.00)").unwrap(), -4500);
        assert_eq!(parse_cents("12").unwrap(), 1200);
        assert_eq!(parse_cents(".99").unwrap(), 99);
        assert_eq!(parse_cents("+3.10").unwrap(), 310);
    }

    #[test]
    fn rejects_bad_input() {
        for s in ["", "abc", "1.234", "1.2.3", "$", "--5", "9999999999999999999"] {
            assert!(parse_cents(s).is_err(), "{s} should fail");
        }
    }

    #[test]
    fn formats() {
        assert_eq!(format_cents(123456), "$1,234.56");
        assert_eq!(format_cents(-5), "-$0.05");
        assert_eq!(format_cents(100000000), "$1,000,000.00");
    }
}
