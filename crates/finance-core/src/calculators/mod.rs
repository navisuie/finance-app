//! The six MVP calculators. Each is a pure function: inputs in, results out,
//! no database access — so they're plain unit tests, not integration tests.
//! Results carry no financial advice, only arithmetic; the UI adds the
//! "this is a projection, not advice" note.

pub mod budget_plan;
pub mod debt_payoff;
pub mod home;
pub mod rent_vs_buy;
pub mod retirement;
pub mod savings_goal;

/// Annual percent to a per-month rate, compounded monthly (e.g. 7.0 -> the
/// monthly rate that compounds to 7% a year).
pub(crate) fn monthly_rate(annual_pct: f64) -> f64 {
    (1.0 + annual_pct / 100.0).powf(1.0 / 12.0) - 1.0
}

/// Future value of `pv` today plus `pmt` at the end of each month for
/// `months`, compounding at `rate` per month. `PFA.md`'s core formula.
pub(crate) fn future_value(pv: f64, pmt: f64, rate: f64, months: u32) -> f64 {
    if months == 0 {
        return pv;
    }
    if rate.abs() < 1e-12 {
        return pv + pmt * months as f64;
    }
    let growth = (1.0 + rate).powi(months as i32);
    pv * growth + pmt * (growth - 1.0) / rate
}

/// The level monthly payment that grows `pv` (plus that payment every month)
/// to exactly `target` after `months`. Solving `future_value` for `pmt`.
pub(crate) fn payment_for_target(pv: f64, target: f64, rate: f64, months: u32) -> f64 {
    if months == 0 {
        return 0.0;
    }
    let needed = target - if rate.abs() < 1e-12 { pv } else { pv * (1.0 + rate).powi(months as i32) };
    if rate.abs() < 1e-12 {
        needed / months as f64
    } else {
        needed * rate / ((1.0 + rate).powi(months as i32) - 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_value_matches_a_month_by_month_simulation() {
        // Cross-check the closed form against a naive month-by-month loop —
        // an independent implementation of the same compounding, so this
        // catches a formula bug without relying on a memorized "reference
        // calculator" number.
        let r = monthly_rate(7.0);
        let (pv, pmt, months) = (10_000.0, 500.0, 120);
        let mut simulated = pv;
        for _ in 0..months {
            simulated = simulated * (1.0 + r) + pmt;
        }
        let fv = future_value(pv, pmt, r, months);
        assert!((fv - simulated).abs() < 0.01, "fv={fv} simulated={simulated}");
    }

    #[test]
    fn payment_for_target_round_trips() {
        let r = monthly_rate(6.0);
        let target = future_value(5_000.0, 300.0, r, 60);
        let pmt = payment_for_target(5_000.0, target, r, 60);
        assert!((pmt - 300.0).abs() < 0.01, "pmt={pmt}");
    }

    #[test]
    fn zero_rate_is_simple_arithmetic() {
        assert_eq!(future_value(1_000.0, 100.0, 0.0, 12), 1_000.0 + 1_200.0);
        assert_eq!(payment_for_target(1_000.0, 2_200.0, 0.0, 12), 100.0);
    }
}
