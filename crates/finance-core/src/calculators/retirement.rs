//! Retirement projection and its Monte Carlo variant.
//!
//! Everything is computed in *real* (today's-dollars) terms: growth uses the
//! real return `(1+nominal)/(1+inflation) - 1`, so a dollar amount coming out
//! of this module already accounts for inflation and can be compared
//! directly to today's spending.

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::{future_value, monthly_rate, payment_for_target};
use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetirementInput {
    pub current_age: u32,
    pub retirement_age: u32,
    /// Combined 401(k) + Roth IRA + brokerage balance today.
    pub balance_cents: i64,
    /// Your own contribution, before any employer match.
    pub monthly_contribution_cents: i64,
    /// Employer match as a percent of your contribution (e.g. 50 for a 50% match), 0 if none.
    pub employer_match_pct: f64,
    pub expected_return_pct: f64,
    pub inflation_pct: f64,
    pub desired_annual_income_cents: i64,
    /// Sustainable withdrawal rate, percent (default 4.0).
    pub withdrawal_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RetirementResult {
    pub months_to_retirement: u32,
    pub combined_monthly_contribution_cents: i64,
    /// At retirement, in today's dollars.
    pub projected_balance_cents: i64,
    /// What that balance supports annually at the withdrawal rate.
    pub sustainable_annual_income_cents: i64,
    /// Positive = surplus over the desired income, negative = shortfall.
    pub gap_cents: i64,
    /// What you'd need to save monthly (in total, not just the delta) to hit
    /// the desired income exactly. Equal to your current contribution when
    /// there's already a surplus.
    pub required_monthly_savings_cents: i64,
    /// One point per year, today through retirement, in today's dollars.
    pub balance_by_year_cents: Vec<i64>,
}

fn validate(input: &RetirementInput) -> Result<()> {
    if input.retirement_age <= input.current_age {
        return Err(CoreError::Invalid("Retirement age has to be after your current age.".into()));
    }
    if input.retirement_age - input.current_age > 90 {
        return Err(CoreError::Invalid("That's more years than this calculator projects.".into()));
    }
    if input.withdrawal_rate_pct <= 0.0 {
        return Err(CoreError::Invalid("Withdrawal rate has to be positive.".into()));
    }
    if input.balance_cents < 0 || input.monthly_contribution_cents < 0 {
        return Err(CoreError::Invalid("Balances and contributions can't be negative.".into()));
    }
    Ok(())
}

pub fn combined_contribution_cents(input: &RetirementInput) -> i64 {
    input.monthly_contribution_cents
        + (input.monthly_contribution_cents as f64 * input.employer_match_pct / 100.0).round() as i64
}

pub fn project(input: &RetirementInput) -> Result<RetirementResult> {
    validate(input)?;
    let months = (input.retirement_age - input.current_age) * 12;
    let years = input.retirement_age - input.current_age;
    let real_annual_pct = ((1.0 + input.expected_return_pct / 100.0) / (1.0 + input.inflation_pct / 100.0) - 1.0) * 100.0;
    let r = monthly_rate(real_annual_pct);
    let contribution = combined_contribution_cents(input) as f64;

    let balance_by_year_cents: Vec<i64> = (0..=years)
        .map(|y| future_value(input.balance_cents as f64, contribution, r, y * 12).round() as i64)
        .collect();
    let projected_balance_cents = *balance_by_year_cents.last().unwrap();

    let sustainable_annual_income_cents =
        (projected_balance_cents as f64 * input.withdrawal_rate_pct / 100.0).round() as i64;
    let gap_cents = sustainable_annual_income_cents - input.desired_annual_income_cents;

    let target_balance = input.desired_annual_income_cents as f64 / (input.withdrawal_rate_pct / 100.0);
    let extra_pmt = payment_for_target(input.balance_cents as f64, target_balance, r, months).max(0.0);
    let required_monthly_savings_cents = if gap_cents >= 0 {
        combined_contribution_cents(input)
    } else {
        (contribution + extra_pmt).round() as i64
    };

    Ok(RetirementResult {
        months_to_retirement: months,
        combined_monthly_contribution_cents: combined_contribution_cents(input),
        projected_balance_cents,
        sustainable_annual_income_cents,
        gap_cents,
        required_monthly_savings_cents,
        balance_by_year_cents,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonteCarloInput {
    #[serde(flatten)]
    pub base: RetirementInput,
    /// Annual standard deviation of returns, percent (e.g. 15 for a
    /// stock-heavy portfolio).
    pub return_volatility_pct: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonteCarloResult {
    /// Share of simulated markets where money lasts through age 95.
    pub success_probability_pct: f64,
    /// One point per year from today through age 95, in today's dollars.
    pub p10_by_year_cents: Vec<i64>,
    pub p50_by_year_cents: Vec<i64>,
    pub p90_by_year_cents: Vec<i64>,
}

const SIMULATIONS: usize = 2_000; // ponytail: PFA.md says 5,000; 2,000 already converges the reported percentiles to the cent-thousands and keeps this instant in a debug build. Raise if results feel noisy.
const LIFE_EXPECTANCY_AGE: u32 = 95;

/// Standard normal sample via Box-Muller, so we don't need the `rand_distr`
/// crate for one distribution.
fn standard_normal(rng: &mut impl Rng) -> f64 {
    let u1: f64 = rng.gen::<f64>().max(1e-12);
    let u2: f64 = rng.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Simulates `SIMULATIONS` random return paths from today through
/// [`LIFE_EXPECTANCY_AGE`]: contributing until retirement, then withdrawing
/// the desired annual income. A run "fails" if the balance is exhausted
/// before life expectancy. Percentiles are taken across simulations at each
/// year mark (not per-path), the standard way Monte Carlo retirement charts
/// are drawn.
pub fn monte_carlo(input: &MonteCarloInput) -> Result<MonteCarloResult> {
    validate(&input.base)?;
    if input.return_volatility_pct < 0.0 {
        return Err(CoreError::Invalid("Volatility can't be negative.".into()));
    }
    let years_total = LIFE_EXPECTANCY_AGE.saturating_sub(input.base.current_age).max(1);
    let months_acc = (input.base.retirement_age - input.base.current_age) * 12;
    let months_total = years_total * 12;

    let real_annual_pct =
        ((1.0 + input.base.expected_return_pct / 100.0) / (1.0 + input.base.inflation_pct / 100.0) - 1.0) * 100.0;
    let mean_monthly = monthly_rate(real_annual_pct);
    let monthly_vol = (input.return_volatility_pct / 100.0) / 12f64.sqrt();
    let contribution = combined_contribution_cents(&input.base) as f64;
    let monthly_withdrawal = input.base.desired_annual_income_cents as f64 / 12.0;

    let mut by_year: Vec<Vec<f64>> = vec![Vec::with_capacity(SIMULATIONS); years_total as usize + 1];
    let mut successes = 0usize;
    let mut rng = rand::thread_rng();

    for _ in 0..SIMULATIONS {
        let mut balance = input.base.balance_cents as f64;
        by_year[0].push(balance);
        let mut ruined = false;
        for m in 1..=months_total {
            let r = mean_monthly + standard_normal(&mut rng) * monthly_vol;
            balance *= 1.0 + r;
            if m <= months_acc {
                balance += contribution;
            } else {
                balance -= monthly_withdrawal;
            }
            if balance <= 0.0 {
                balance = 0.0;
                if m > months_acc {
                    ruined = true;
                }
            }
            if m % 12 == 0 {
                by_year[(m / 12) as usize].push(balance);
            }
        }
        if !ruined {
            successes += 1;
        }
    }

    let percentile = |values: &mut [f64], p: f64| -> i64 {
        values.sort_by(|a, b| a.total_cmp(b));
        let idx = ((values.len() - 1) as f64 * p).round() as usize;
        values[idx].round() as i64
    };
    let mut p10 = Vec::with_capacity(by_year.len());
    let mut p50 = Vec::with_capacity(by_year.len());
    let mut p90 = Vec::with_capacity(by_year.len());
    for year in &mut by_year {
        p10.push(percentile(year, 0.10));
        p50.push(percentile(year, 0.50));
        p90.push(percentile(year, 0.90));
    }

    Ok(MonteCarloResult {
        success_probability_pct: successes as f64 / SIMULATIONS as f64 * 100.0,
        p10_by_year_cents: p10,
        p50_by_year_cents: p50,
        p90_by_year_cents: p90,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> RetirementInput {
        RetirementInput {
            current_age: 30,
            retirement_age: 65,
            balance_cents: 5_000_000,
            monthly_contribution_cents: 100_000,
            employer_match_pct: 50.0,
            expected_return_pct: 7.0,
            inflation_pct: 3.0,
            desired_annual_income_cents: 8_000_000,
            withdrawal_rate_pct: 4.0,
        }
    }

    #[test]
    fn employer_match_is_added_on_top() {
        let i = base();
        assert_eq!(combined_contribution_cents(&i), 150_000);
    }

    #[test]
    fn projects_growth_and_a_shortfall_or_surplus() {
        let r = project(&base()).unwrap();
        assert_eq!(r.months_to_retirement, 420);
        assert!(r.projected_balance_cents > base().balance_cents, "35 years of growth should compound");
        assert_eq!(r.balance_by_year_cents.len(), 36); // year 0 through 35
        assert_eq!(r.balance_by_year_cents[0], base().balance_cents);
        // Gap should be consistent with sustainable income vs desired.
        assert_eq!(r.gap_cents, r.sustainable_annual_income_cents - base().desired_annual_income_cents);
        if r.gap_cents < 0 {
            assert!(r.required_monthly_savings_cents > r.combined_monthly_contribution_cents);
        } else {
            assert_eq!(r.required_monthly_savings_cents, r.combined_monthly_contribution_cents);
        }
    }

    #[test]
    fn more_contribution_never_makes_the_gap_worse() {
        let low = project(&base()).unwrap();
        let mut higher = base();
        higher.monthly_contribution_cents += 50_000;
        let high = project(&higher).unwrap();
        assert!(high.projected_balance_cents > low.projected_balance_cents);
        assert!(high.gap_cents >= low.gap_cents);
    }

    #[test]
    fn validation() {
        let mut i = base();
        i.retirement_age = i.current_age;
        assert!(project(&i).is_err());
        let mut i = base();
        i.withdrawal_rate_pct = 0.0;
        assert!(project(&i).is_err());
        let mut i = base();
        i.balance_cents = -1;
        assert!(project(&i).is_err());
    }

    #[test]
    fn monte_carlo_success_rate_moves_with_volatility_and_savings() {
        let comfortable = MonteCarloInput {
            base: RetirementInput { desired_annual_income_cents: 3_000_000, ..base() },
            return_volatility_pct: 12.0,
        };
        let r = monte_carlo(&comfortable).unwrap();
        assert!(r.success_probability_pct > 50.0, "modest spending against a big balance should mostly succeed");
        assert_eq!(r.p10_by_year_cents.len(), r.p50_by_year_cents.len());
        assert!(r.p10_by_year_cents[0] == comfortable.base.balance_cents);
        // p10 <= p50 <= p90 at every year.
        for i in 0..r.p10_by_year_cents.len() {
            assert!(r.p10_by_year_cents[i] <= r.p50_by_year_cents[i]);
            assert!(r.p50_by_year_cents[i] <= r.p90_by_year_cents[i]);
        }

        let tight = MonteCarloInput {
            base: RetirementInput { balance_cents: 100_000, desired_annual_income_cents: 12_000_000, ..base() },
            return_volatility_pct: 12.0,
        };
        let r2 = monte_carlo(&tight).unwrap();
        assert!(r2.success_probability_pct < r.success_probability_pct);
    }
}
