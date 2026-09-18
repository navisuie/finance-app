//! Avalanche (highest APR first) vs. snowball (smallest balance first) for
//! paying off several debts at once, sharing one extra-payment budget.
//!
//! Reuses `debt::monthly_interest`/`add_months` — the same per-month
//! interest accrual already tested against a real amortization schedule for
//! the single-debt case.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::debt::{add_months, monthly_interest};
use crate::error::{CoreError, Result};
use crate::util::today;

const MAX_MONTHS: u32 = 600; // 50 years, matches debt.rs

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtInput {
    pub name: String,
    pub balance_cents: i64,
    pub apr_bps: i64,
    pub minimum_payment_cents: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebtPayoffInput {
    pub debts: Vec<DebtInput>,
    pub extra_monthly_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StrategyResult {
    pub months: u32,
    pub payoff_on: String,
    pub total_interest_cents: i64,
    pub total_paid_cents: i64,
    /// The order debts get the extra payment applied to.
    pub order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DebtPayoffResult {
    pub avalanche: StrategyResult,
    pub snowball: StrategyResult,
}

fn validate(input: &DebtPayoffInput) -> Result<()> {
    if input.debts.is_empty() {
        return Err(CoreError::Invalid("Add at least one debt.".into()));
    }
    if input.extra_monthly_cents < 0 {
        return Err(CoreError::Invalid("Extra payment can't be negative.".into()));
    }
    for d in &input.debts {
        if d.balance_cents < 0 || d.apr_bps < 0 || d.minimum_payment_cents < 0 {
            return Err(CoreError::Invalid(format!("{}: amounts can't be negative.", d.name)));
        }
    }
    Ok(())
}

/// Simulates one strategy: `order` is the index order the extra budget cascades through.
fn simulate(debts: &[DebtInput], extra_cents: i64, order: &[usize], from: NaiveDate) -> StrategyResult {
    let total_budget: i64 = debts.iter().map(|d| d.minimum_payment_cents).sum::<i64>() + extra_cents;
    let mut balances: Vec<i64> = debts.iter().map(|d| d.balance_cents).collect();
    let starting_total: i64 = balances.iter().sum();
    let mut total_interest = 0i64;
    let mut months = 0u32;

    while balances.iter().any(|&b| b > 0) && months < MAX_MONTHS {
        months += 1;
        for (i, d) in debts.iter().enumerate() {
            if balances[i] > 0 {
                let interest = monthly_interest(balances[i], d.apr_bps);
                balances[i] += interest;
                total_interest += interest;
            }
        }
        let mut remaining = total_budget;
        for (i, d) in debts.iter().enumerate() {
            if balances[i] > 0 {
                let pay = d.minimum_payment_cents.min(balances[i]).min(remaining);
                balances[i] -= pay;
                remaining -= pay;
            }
        }
        for &i in order {
            if remaining <= 0 {
                break;
            }
            if balances[i] > 0 {
                let pay = balances[i].min(remaining);
                balances[i] -= pay;
                remaining -= pay;
            }
        }
    }

    StrategyResult {
        months,
        payoff_on: add_months(from, months).format("%Y-%m-%d").to_string(),
        total_interest_cents: total_interest,
        total_paid_cents: starting_total + total_interest,
        order: order.iter().map(|&i| debts[i].name.clone()).collect(),
    }
}

pub fn project(input: &DebtPayoffInput) -> Result<DebtPayoffResult> {
    validate(input)?;
    let from = NaiveDate::parse_from_str(&today(), "%Y-%m-%d").unwrap_or_default();

    let mut avalanche_order: Vec<usize> = (0..input.debts.len()).collect();
    avalanche_order.sort_by(|&a, &b| input.debts[b].apr_bps.cmp(&input.debts[a].apr_bps));

    let mut snowball_order: Vec<usize> = (0..input.debts.len()).collect();
    snowball_order.sort_by_key(|&i| input.debts[i].balance_cents);

    Ok(DebtPayoffResult {
        avalanche: simulate(&input.debts, input.extra_monthly_cents, &avalanche_order, from),
        snowball: simulate(&input.debts, input.extra_monthly_cents, &snowball_order, from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn debt(name: &str, balance: i64, apr_bps: i64, minimum: i64) -> DebtInput {
        DebtInput { name: name.into(), balance_cents: balance, apr_bps, minimum_payment_cents: minimum }
    }

    fn input() -> DebtPayoffInput {
        DebtPayoffInput {
            debts: vec![
                debt("Card A", 500_000, 2200, 25_000),  // high APR, small balance
                debt("Car loan", 1_500_000, 649, 45_000), // low APR, big balance
                debt("Card B", 200_000, 1800, 10_000),
            ],
            extra_monthly_cents: 20_000,
        }
    }

    #[test]
    fn avalanche_orders_by_highest_apr() {
        let r = project(&input()).unwrap();
        assert_eq!(r.avalanche.order, vec!["Card A", "Card B", "Car loan"]);
    }

    #[test]
    fn snowball_orders_by_smallest_balance() {
        let r = project(&input()).unwrap();
        assert_eq!(r.snowball.order, vec!["Card B", "Card A", "Car loan"]);
    }

    #[test]
    fn avalanche_never_costs_more_interest_than_snowball() {
        // Classic result: avalanche minimizes total interest (snowball may
        // finish sooner if it front-loads small balances, but never cheaper).
        let r = project(&input()).unwrap();
        assert!(r.avalanche.total_interest_cents <= r.snowball.total_interest_cents);
    }

    #[test]
    fn more_extra_payment_pays_off_faster_and_cheaper() {
        let base = project(&input()).unwrap();
        let mut more = input();
        more.extra_monthly_cents = 100_000;
        let faster = project(&more).unwrap();
        assert!(faster.avalanche.months < base.avalanche.months);
        assert!(faster.avalanche.total_interest_cents < base.avalanche.total_interest_cents);
    }

    #[test]
    fn validation() {
        let mut i = input();
        i.debts.clear();
        assert!(project(&i).is_err());
        let mut i = input();
        i.extra_monthly_cents = -1;
        assert!(project(&i).is_err());
    }
}
