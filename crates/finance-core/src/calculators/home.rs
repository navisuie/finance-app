//! Home affordability under the 28/36 debt-to-income rule.
//!
//! Property tax and PMI both scale with the home price we're solving for,
//! which makes the "max price" equation circular (PMI also depends on the
//! down-payment ratio at that price). Rather than untangle that with
//! conditional algebra, we binary-search the price: monotonic in price, so
//! the search converges to the cent in ~60 steps.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Estimated annual PMI rate on the loan balance while equity is under 20%.
const PMI_ANNUAL_RATE_PCT: f64 = 0.5;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeAffordabilityInput {
    pub gross_annual_income_cents: i64,
    /// Minimum payments on other debts (cards, car, student loans).
    pub monthly_debts_cents: i64,
    pub down_payment_cents: i64,
    pub mortgage_rate_pct: f64,
    pub term_years: u32,
    /// Annual, percent of home value.
    pub property_tax_pct: f64,
    pub monthly_insurance_cents: i64,
    pub monthly_hoa_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentBreakdown {
    pub principal_interest_cents: i64,
    pub tax_cents: i64,
    pub insurance_cents: i64,
    pub hoa_cents: i64,
    pub pmi_cents: i64,
    pub total_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HomeAffordabilityResult {
    pub max_home_price_cents: i64,
    pub max_loan_cents: i64,
    pub monthly_payment: PaymentBreakdown,
    pub down_payment_pct: f64,
}

/// Monthly PITI (+PMI) for a given home price, at this input's rate/term/etc.
fn monthly_payment_for_price(input: &HomeAffordabilityInput, price: f64) -> PaymentBreakdown {
    let loan = (price - input.down_payment_cents as f64).max(0.0);
    let r = input.mortgage_rate_pct / 100.0 / 12.0;
    let n = (input.term_years * 12) as i32;
    let principal_interest = if loan <= 0.0 {
        0.0
    } else if r.abs() < 1e-12 {
        loan / n as f64
    } else {
        loan * r * (1.0 + r).powi(n) / ((1.0 + r).powi(n) - 1.0)
    };
    let tax = price * input.property_tax_pct / 100.0 / 12.0;
    let equity_pct = if price > 0.0 { input.down_payment_cents as f64 / price } else { 1.0 };
    let pmi = if equity_pct < 0.20 { loan * PMI_ANNUAL_RATE_PCT / 100.0 / 12.0 } else { 0.0 };
    let insurance = input.monthly_insurance_cents as f64;
    let hoa = input.monthly_hoa_cents as f64;
    PaymentBreakdown {
        principal_interest_cents: principal_interest.round() as i64,
        tax_cents: tax.round() as i64,
        insurance_cents: insurance.round() as i64,
        hoa_cents: hoa.round() as i64,
        pmi_cents: pmi.round() as i64,
        total_cents: (principal_interest + tax + insurance + pmi + hoa).round() as i64,
    }
}

pub fn project(input: &HomeAffordabilityInput) -> Result<HomeAffordabilityResult> {
    if input.gross_annual_income_cents <= 0 {
        return Err(CoreError::Invalid("Gross income has to be positive.".into()));
    }
    if input.term_years == 0 {
        return Err(CoreError::Invalid("Term has to be at least a year.".into()));
    }
    if input.down_payment_cents < 0 || input.monthly_debts_cents < 0 {
        return Err(CoreError::Invalid("Amounts can't be negative.".into()));
    }
    let gross_monthly = input.gross_annual_income_cents as f64 / 12.0;
    let max_from_front = gross_monthly * 0.28;
    let max_from_back = gross_monthly * 0.36 - input.monthly_debts_cents as f64;
    let max_piti = max_from_front.min(max_from_back).max(0.0);

    // Binary search the largest price whose PITI+PMI doesn't exceed max_piti.
    let mut lo = 0.0f64;
    let mut hi = (input.gross_annual_income_cents as f64) * 15.0 + input.down_payment_cents as f64 + 1.0;
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if monthly_payment_for_price(input, mid).total_cents as f64 <= max_piti {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let max_home_price_cents = lo.round() as i64;
    let monthly_payment = monthly_payment_for_price(input, lo);
    let down_payment_pct =
        if max_home_price_cents > 0 { input.down_payment_cents as f64 / max_home_price_cents as f64 * 100.0 } else { 0.0 };

    Ok(HomeAffordabilityResult {
        max_home_price_cents,
        max_loan_cents: (max_home_price_cents - input.down_payment_cents).max(0),
        monthly_payment,
        down_payment_pct,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> HomeAffordabilityInput {
        HomeAffordabilityInput {
            gross_annual_income_cents: 12_000_000, // $120k
            monthly_debts_cents: 50_000,           // $500 car payment
            down_payment_cents: 8_000_000,         // $80k, 20%+ at a reasonable price
            mortgage_rate_pct: 6.5,
            term_years: 30,
            property_tax_pct: 1.2,
            monthly_insurance_cents: 15_000,
            monthly_hoa_cents: 0,
        }
    }

    #[test]
    fn max_price_respects_28_36_rule() {
        let r = project(&input()).unwrap();
        let gross_monthly = input().gross_annual_income_cents as f64 / 12.0;
        assert!(r.monthly_payment.total_cents as f64 <= gross_monthly * 0.28 + 1.0);
        assert!(
            r.monthly_payment.total_cents as f64 + input().monthly_debts_cents as f64 <= gross_monthly * 0.36 + 1.0
        );
        // A meaningfully higher price should not fit the budget.
        let over = monthly_payment_for_price(&input(), r.max_home_price_cents as f64 + 100_000.0);
        assert!(over.total_cents > r.monthly_payment.total_cents);
    }

    #[test]
    fn pmi_applies_under_20_percent_down() {
        let mut i = input();
        i.down_payment_cents = 1_000_000; // 10% of a $1M home
        let thin = monthly_payment_for_price(&i, 100_000_000.0);
        assert!(thin.pmi_cents > 0, "should carry PMI with a thin down payment");

        i.down_payment_cents = 25_000_000; // 25% of the same $1M home
        let solid = monthly_payment_for_price(&i, 100_000_000.0);
        assert_eq!(solid.pmi_cents, 0);
    }

    #[test]
    fn more_debt_lowers_affordability() {
        let baseline = project(&input()).unwrap();
        let mut i = input();
        i.monthly_debts_cents = 300_000; // a hefty car/student loan load
        let strained = project(&i).unwrap();
        assert!(strained.max_home_price_cents < baseline.max_home_price_cents);
    }

    #[test]
    fn validation() {
        let mut i = input();
        i.gross_annual_income_cents = 0;
        assert!(project(&i).is_err());
        let mut i = input();
        i.term_years = 0;
        assert!(project(&i).is_err());
    }
}
