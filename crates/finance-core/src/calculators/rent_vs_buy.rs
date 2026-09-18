//! Net wealth under renting-and-investing vs. buying, year by year.
//!
//! Simplifications (ponytail: real closing/selling costs and mortgage
//! interest deductions would need a fuller tax model; add if this ever needs
//! to be precise rather than directional): no closing costs, no selling
//! costs at the end, no tax effects. The renter's portfolio is credited or
//! debited monthly by (buying cost − renting cost), which can go negative in
//! a month renting is pricier — treated as a smaller contribution, not a
//! literal withdrawal restriction.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RentVsBuyInput {
    pub monthly_rent_cents: i64,
    pub annual_rent_growth_pct: f64,
    pub home_price_cents: i64,
    pub down_payment_cents: i64,
    pub mortgage_rate_pct: f64,
    pub term_years: u32,
    /// Annual, percent of current home value.
    pub maintenance_pct: f64,
    pub annual_appreciation_pct: f64,
    pub years_staying: u32,
    pub investment_return_pct: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct YearlyWealth {
    pub year: u32,
    pub rent_net_worth_cents: i64,
    pub buy_net_worth_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RentVsBuyResult {
    pub by_year: Vec<YearlyWealth>,
    /// First year buying's net worth overtakes renting's, if it happens within `years_staying`.
    pub break_even_year: Option<u32>,
}

fn mortgage_balance(loan: f64, monthly_rate: f64, term_months: u32, months_elapsed: u32) -> f64 {
    if months_elapsed >= term_months {
        return 0.0;
    }
    if monthly_rate.abs() < 1e-12 {
        return loan * (term_months - months_elapsed) as f64 / term_months as f64;
    }
    let n = term_months as f64;
    let k = months_elapsed as f64;
    let growth_n = (1.0 + monthly_rate).powf(n);
    let growth_k = (1.0 + monthly_rate).powf(k);
    (loan * (growth_n - growth_k) / (growth_n - 1.0)).max(0.0)
}

fn monthly_pi(loan: f64, monthly_rate: f64, term_months: u32) -> f64 {
    if monthly_rate.abs() < 1e-12 {
        return loan / term_months as f64;
    }
    let g = (1.0 + monthly_rate).powi(term_months as i32);
    loan * monthly_rate * g / (g - 1.0)
}

pub fn project(input: &RentVsBuyInput) -> Result<RentVsBuyResult> {
    if input.years_staying == 0 {
        return Err(CoreError::Invalid("Years staying has to be at least 1.".into()));
    }
    if input.home_price_cents <= 0 || input.monthly_rent_cents <= 0 {
        return Err(CoreError::Invalid("Rent and home price have to be positive.".into()));
    }
    if input.down_payment_cents < 0 || input.down_payment_cents > input.home_price_cents {
        return Err(CoreError::Invalid("Down payment has to be between 0 and the home price.".into()));
    }

    let loan = (input.home_price_cents - input.down_payment_cents) as f64;
    let mortgage_monthly_rate = input.mortgage_rate_pct / 100.0 / 12.0;
    let term_months = input.term_years.max(1) * 12;
    let pi = monthly_pi(loan, mortgage_monthly_rate, term_months);
    let invest_monthly_rate = (1.0 + input.investment_return_pct / 100.0).powf(1.0 / 12.0) - 1.0;

    let mut portfolio = input.down_payment_cents as f64;
    let mut by_year = Vec::with_capacity(input.years_staying as usize + 1);
    by_year.push(YearlyWealth {
        year: 0,
        rent_net_worth_cents: portfolio.round() as i64,
        buy_net_worth_cents: input.home_price_cents - loan as i64,
    });
    let mut break_even_year = None;

    for month in 1..=(input.years_staying * 12) {
        let year_index = (month - 1) / 12; // 0-based year this month falls in
        let home_value = input.home_price_cents as f64 * (1.0 + input.annual_appreciation_pct / 100.0).powi(year_index as i32 + 1);
        let rent = input.monthly_rent_cents as f64 * (1.0 + input.annual_rent_growth_pct / 100.0).powi(year_index as i32);
        let maintenance = home_value * input.maintenance_pct / 100.0 / 12.0;
        let buy_cash_cost = pi + maintenance;

        portfolio = portfolio * (1.0 + invest_monthly_rate) + (buy_cash_cost - rent);

        if month % 12 == 0 {
            let year = month / 12;
            let balance = mortgage_balance(loan, mortgage_monthly_rate, term_months, month);
            let home_value_year_end = input.home_price_cents as f64 * (1.0 + input.annual_appreciation_pct / 100.0).powi(year as i32);
            let buy_net_worth = (home_value_year_end - balance).round() as i64;
            let rent_net_worth = portfolio.round() as i64;
            if break_even_year.is_none() && buy_net_worth >= rent_net_worth {
                break_even_year = Some(year);
            }
            by_year.push(YearlyWealth { year, rent_net_worth_cents: rent_net_worth, buy_net_worth_cents: buy_net_worth });
        }
    }

    Ok(RentVsBuyResult { by_year, break_even_year })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> RentVsBuyInput {
        RentVsBuyInput {
            monthly_rent_cents: 250_000,
            annual_rent_growth_pct: 3.0,
            home_price_cents: 50_000_000,
            down_payment_cents: 10_000_000,
            mortgage_rate_pct: 6.5,
            term_years: 30,
            maintenance_pct: 1.0,
            annual_appreciation_pct: 3.5,
            years_staying: 10,
            investment_return_pct: 7.0,
        }
    }

    #[test]
    fn buying_eventually_builds_more_equity_with_strong_appreciation() {
        let r = project(&input()).unwrap();
        assert_eq!(r.by_year.len(), 11); // year 0..=10
        assert_eq!(r.by_year[0].year, 0);
        // Home equity should grow every year (appreciation + paydown).
        for w in r.by_year.windows(2) {
            assert!(w[1].buy_net_worth_cents > w[0].buy_net_worth_cents);
        }
    }

    #[test]
    fn richer_appreciation_favors_buying_sooner() {
        let mut slow = input();
        slow.annual_appreciation_pct = 0.5;
        let mut fast = input();
        fast.annual_appreciation_pct = 8.0;
        let slow_r = project(&slow).unwrap();
        let fast_r = project(&fast).unwrap();
        let last = slow_r.by_year.len() - 1;
        assert!(fast_r.by_year[last].buy_net_worth_cents > slow_r.by_year[last].buy_net_worth_cents);
    }

    #[test]
    fn validation() {
        let mut i = input();
        i.years_staying = 0;
        assert!(project(&i).is_err());
        let mut i = input();
        i.down_payment_cents = i.home_price_cents + 1;
        assert!(project(&i).is_err());
    }
}
