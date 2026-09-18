//! How much to save monthly to hit a target by a date, and — at whatever
//! you're actually putting away today — when you'd get there.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use super::{future_value, monthly_rate, payment_for_target};
use crate::error::{CoreError, Result};
use crate::util::{parse_date, today};

const MAX_MONTHS: u32 = 1200; // 100 years, matches the calculator's practical horizon.

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavingsGoalInput {
    pub target_cents: i64,
    pub target_date: String,
    pub current_savings_cents: i64,
    pub hysa_apy_pct: f64,
    /// What you're depositing today, to project a completion date at that pace.
    pub current_monthly_deposit_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavingsGoalResult {
    pub months_to_target_date: u32,
    /// What you'd need to deposit monthly to land on the target date exactly.
    pub required_monthly_deposit_cents: i64,
    /// `None` means the current deposit never gets there (or already there).
    pub projected_completion_date: Option<String>,
    pub already_reached: bool,
}

fn months_between(from: &str, to: &str) -> Result<u32> {
    let from = NaiveDate::parse_from_str(from, "%Y-%m-%d").map_err(|_| CoreError::Invalid("Bad date.".into()))?;
    let to = NaiveDate::parse_from_str(to, "%Y-%m-%d").map_err(|_| CoreError::Invalid("Bad date.".into()))?;
    let months = (to.year() - from.year()) * 12 + (to.month() as i32 - from.month() as i32);
    Ok(months.max(0) as u32)
}

pub fn project(input: &SavingsGoalInput) -> Result<SavingsGoalResult> {
    if input.target_cents <= 0 {
        return Err(CoreError::Invalid("Target amount has to be positive.".into()));
    }
    if input.current_savings_cents < 0 {
        return Err(CoreError::Invalid("Current savings can't be negative.".into()));
    }
    let target_date = parse_date(&input.target_date)?;
    let day = today();
    let months_to_target_date = months_between(&day, &target_date)?;
    let r = monthly_rate(input.hysa_apy_pct);
    let already_reached = input.current_savings_cents >= input.target_cents;

    let required_monthly_deposit_cents = if already_reached || months_to_target_date == 0 {
        0
    } else {
        payment_for_target(
            input.current_savings_cents as f64,
            input.target_cents as f64,
            r,
            months_to_target_date,
        )
        .max(0.0)
        .round() as i64
    };

    let projected_completion_date = if already_reached {
        Some(day)
    } else if input.current_monthly_deposit_cents <= 0 && r <= 0.0 {
        None
    } else {
        let mut balance = input.current_savings_cents as f64;
        let mut m = 0u32;
        while balance < input.target_cents as f64 && m < MAX_MONTHS {
            balance = future_value(balance, input.current_monthly_deposit_cents as f64, r, 1);
            m += 1;
        }
        if balance >= input.target_cents as f64 {
            NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.checked_add_months(chrono::Months::new(m)))
                .map(|d| d.format("%Y-%m-%d").to_string())
        } else {
            None
        }
    };

    Ok(SavingsGoalResult {
        months_to_target_date,
        required_monthly_deposit_cents,
        projected_completion_date,
        already_reached,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> SavingsGoalInput {
        SavingsGoalInput {
            target_cents: 2_000_000,
            target_date: "2028-09-17".into(), // 2 years out from the fixed "today" below
            current_savings_cents: 500_000,
            hysa_apy_pct: 4.5,
            current_monthly_deposit_cents: 50_000,
        }
    }

    #[test]
    fn required_deposit_reaches_target_on_time() {
        let r = project(&input()).unwrap();
        assert!(r.required_monthly_deposit_cents > 0);
        // Depositing exactly the required amount should land within a
        // rounding cent of the target by the target date.
        let rate = monthly_rate(input().hysa_apy_pct);
        let fv = future_value(
            input().current_savings_cents as f64,
            r.required_monthly_deposit_cents as f64,
            rate,
            r.months_to_target_date,
        );
        assert!((fv - input().target_cents as f64).abs() < 200.0, "fv={fv}");
    }

    #[test]
    fn already_reached_needs_nothing() {
        let mut i = input();
        i.current_savings_cents = i.target_cents;
        let r = project(&i).unwrap();
        assert!(r.already_reached);
        assert_eq!(r.required_monthly_deposit_cents, 0);
        assert!(r.projected_completion_date.is_some());
    }

    #[test]
    fn zero_pace_never_arrives_without_interest() {
        let mut i = input();
        i.current_monthly_deposit_cents = 0;
        i.hysa_apy_pct = 0.0;
        let r = project(&i).unwrap();
        assert_eq!(r.projected_completion_date, None);
    }

    #[test]
    fn validation() {
        let mut i = input();
        i.target_cents = 0;
        assert!(project(&i).is_err());
        let mut i = input();
        i.current_savings_cents = -1;
        assert!(project(&i).is_err());
        let mut i = input();
        i.target_date = "not a date".into();
        assert!(project(&i).is_err());
    }
}
