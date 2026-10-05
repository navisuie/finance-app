//! A budget from a salary and whatever fixed costs you already know about.
//!
//! Needs no account history: you give a salary and, say, a $700 car payment,
//! and the rest of the month gets filled in against a 50/30/20 split (needs,
//! wants, savings) that you can change. Category names match the ones the app
//! seeds, so the result maps onto real budgets.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Which third of the rule a line belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Needs,
    Wants,
    Savings,
}

/// How what's left over gets split inside each bucket, once the fixed costs
/// are taken out. Weights are relative, so dropping a covered category just
/// spreads its share across the rest.
const NEEDS_WEIGHTS: &[(&str, f64)] = &[
    ("Housing", 0.45),
    ("Groceries", 0.20),
    ("Transportation", 0.13),
    ("Utilities", 0.10),
    ("Insurance", 0.07),
    ("Health", 0.05),
];
const WANTS_WEIGHTS: &[(&str, f64)] =
    &[("Dining", 0.35), ("Shopping", 0.25), ("Entertainment", 0.20), ("Subscriptions", 0.10), ("Travel", 0.10)];
const SAVINGS_WEIGHTS: &[(&str, f64)] =
    &[("Retirement", 0.45), ("Emergency fund", 0.40), ("Other savings", 0.15)];

/// A cost you already know, e.g. rent or a car payment.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedExpense {
    pub label: String,
    pub monthly_cents: i64,
    pub bucket: Bucket,
    /// The suggested category this replaces, if any (a $700 car payment set to
    /// "Transportation" takes that line over instead of being added on top).
    pub covers: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPlanInput {
    pub gross_annual_income_cents: i64,
    /// Everything withheld from the paycheck — federal, state, payroll tax,
    /// health premiums. Used to estimate take-home.
    pub tax_rate_pct: f64,
    pub fixed_expenses: Vec<FixedExpense>,
    /// The rule to fill the rest against; 50/30/20 by default.
    pub needs_pct: f64,
    pub wants_pct: f64,
    pub savings_pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineSource {
    /// A cost you entered.
    Fixed,
    /// Filled in from what's left in that bucket.
    Suggested,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanLine {
    pub bucket: Bucket,
    pub label: String,
    pub monthly_cents: i64,
    pub source: LineSource,
    /// The spending category this line belongs to, when it maps to one, so
    /// the plan can be written into real budgets. `None` for a fixed cost
    /// that covers nothing and for the savings lines, which aren't spending
    /// categories in this app.
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BucketSummary {
    pub bucket: Bucket,
    pub target_cents: i64,
    /// What your fixed costs already take out of this bucket.
    pub fixed_cents: i64,
    /// Target minus fixed; negative means the fixed costs blow the bucket.
    pub remaining_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPlanResult {
    pub monthly_take_home_cents: i64,
    pub buckets: Vec<BucketSummary>,
    pub lines: Vec<PlanLine>,
    pub total_planned_cents: i64,
    /// Take-home minus everything planned. Rounding aside, this is 0 unless a
    /// bucket is over its target.
    pub unallocated_cents: i64,
    /// True when fixed costs exceed a bucket's share, so the rest can't be filled in.
    pub over_allocated: bool,
}

/// Round to the nearest $5 so the numbers read like a budget, not a division.
fn round_to_5(cents: i64) -> i64 {
    (cents as f64 / 500.0).round() as i64 * 500
}

fn validate(input: &BudgetPlanInput) -> Result<()> {
    if input.gross_annual_income_cents <= 0 {
        return Err(CoreError::Invalid("Enter your salary to build a budget.".into()));
    }
    if !(0.0..100.0).contains(&input.tax_rate_pct) {
        return Err(CoreError::Invalid("Tax rate has to be between 0 and 100 percent.".into()));
    }
    let total = input.needs_pct + input.wants_pct + input.savings_pct;
    if (total - 100.0).abs() > 0.01 {
        return Err(CoreError::Invalid(format!(
            "The needs, wants and savings shares have to add up to 100% (they add up to {total:.0}%)."
        )));
    }
    for f in &input.fixed_expenses {
        if f.monthly_cents < 0 {
            return Err(CoreError::Invalid(format!("{}: a cost can't be negative.", f.label)));
        }
    }
    Ok(())
}

/// Spread `amount` across `weights`, skipping anything already covered by a
/// fixed cost and re-spreading its share over the rest.
fn spread(amount: i64, weights: &[(&str, f64)], covered: &[String], bucket: Bucket) -> Vec<PlanLine> {
    let open: Vec<&(&str, f64)> =
        weights.iter().filter(|(name, _)| !covered.iter().any(|c| c == name)).collect();
    let total_weight: f64 = open.iter().map(|(_, w)| w).sum();
    if amount <= 0 || total_weight <= 0.0 {
        return Vec::new();
    }
    open.iter()
        .map(|(name, weight)| PlanLine {
            bucket,
            label: (*name).to_string(),
            monthly_cents: round_to_5((amount as f64 * (weight / total_weight)) as i64),
            source: LineSource::Suggested,
            // Savings lines are labels, not categories this app budgets.
            category: (bucket != Bucket::Savings).then(|| (*name).to_string()),
        })
        .filter(|l| l.monthly_cents > 0)
        .collect()
}

pub fn project(input: &BudgetPlanInput) -> Result<BudgetPlanResult> {
    validate(input)?;

    let monthly_take_home_cents =
        (input.gross_annual_income_cents as f64 / 12.0 * (1.0 - input.tax_rate_pct / 100.0)).round() as i64;

    let mut lines: Vec<PlanLine> = Vec::new();
    let mut buckets = Vec::new();
    let mut over_allocated = false;

    for (bucket, pct, weights) in [
        (Bucket::Needs, input.needs_pct, NEEDS_WEIGHTS),
        (Bucket::Wants, input.wants_pct, WANTS_WEIGHTS),
        (Bucket::Savings, input.savings_pct, SAVINGS_WEIGHTS),
    ] {
        let target_cents = (monthly_take_home_cents as f64 * pct / 100.0).round() as i64;
        let mine: Vec<&FixedExpense> = input.fixed_expenses.iter().filter(|f| f.bucket == bucket).collect();
        let fixed_cents: i64 = mine.iter().map(|f| f.monthly_cents).sum();
        let covered: Vec<String> = mine.iter().filter_map(|f| f.covers.clone()).collect();

        for f in &mine {
            lines.push(PlanLine {
                bucket,
                label: f.label.trim().to_string(),
                monthly_cents: f.monthly_cents,
                source: LineSource::Fixed,
                category: if bucket == Bucket::Savings { None } else { f.covers.clone() },
            });
        }
        let remaining_cents = target_cents - fixed_cents;
        if remaining_cents < 0 {
            over_allocated = true;
        }
        lines.extend(spread(remaining_cents, weights, &covered, bucket));
        buckets.push(BucketSummary { bucket, target_cents, fixed_cents, remaining_cents });
    }

    let total_planned_cents: i64 = lines.iter().map(|l| l.monthly_cents).sum();
    Ok(BudgetPlanResult {
        monthly_take_home_cents,
        buckets,
        lines,
        total_planned_cents,
        unallocated_cents: monthly_take_home_cents - total_planned_cents,
        over_allocated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(label: &str, cents: i64, bucket: Bucket, covers: Option<&str>) -> FixedExpense {
        FixedExpense {
            label: label.into(),
            monthly_cents: cents,
            bucket,
            covers: covers.map(|c| c.to_string()),
        }
    }

    fn input(fixed_expenses: Vec<FixedExpense>) -> BudgetPlanInput {
        BudgetPlanInput {
            gross_annual_income_cents: 9_600_000, // $96,000
            tax_rate_pct: 25.0,
            fixed_expenses,
            needs_pct: 50.0,
            wants_pct: 30.0,
            savings_pct: 20.0,
        }
    }

    #[test]
    fn splits_take_home_by_the_rule() {
        let r = project(&input(vec![])).unwrap();
        // $96k gross, 25% withheld -> $6,000 a month take-home.
        assert_eq!(r.monthly_take_home_cents, 600_000);
        assert_eq!(r.buckets[0].target_cents, 300_000); // needs, 50%
        assert_eq!(r.buckets[1].target_cents, 180_000); // wants, 30%
        assert_eq!(r.buckets[2].target_cents, 120_000); // savings, 20%
        assert!(!r.over_allocated);
        // Everything is allocated, give or take rounding to the nearest $5.
        assert!(r.unallocated_cents.abs() <= 2_000, "unallocated={}", r.unallocated_cents);
    }

    #[test]
    fn a_known_car_payment_takes_over_its_category() {
        let r = project(&input(vec![fixed("Car payment", 70_000, Bucket::Needs, Some("Transportation"))])).unwrap();
        let car = r.lines.iter().find(|l| l.label == "Car payment").unwrap();
        assert_eq!(car.monthly_cents, 70_000);
        assert_eq!(car.source, LineSource::Fixed);
        // Transportation isn't suggested on top of it.
        assert!(!r.lines.iter().any(|l| l.label == "Transportation"));
        // The rest of needs is filled from what's left: $3,000 - $700.
        let needs = &r.buckets[0];
        assert_eq!(needs.fixed_cents, 70_000);
        assert_eq!(needs.remaining_cents, 230_000);
        let suggested_needs: i64 = r
            .lines
            .iter()
            .filter(|l| l.bucket == Bucket::Needs && l.source == LineSource::Suggested)
            .map(|l| l.monthly_cents)
            .sum();
        assert!((suggested_needs - 230_000).abs() <= 2_000, "suggested={suggested_needs}");
    }

    #[test]
    fn lines_carry_the_category_they_would_be_budgeted_under() {
        let r = project(&input(vec![
            fixed("Car payment", 70_000, Bucket::Needs, Some("Transportation")),
            fixed("Gym", 5_000, Bucket::Wants, None),
        ]))
        .unwrap();
        let car = r.lines.iter().find(|l| l.label == "Car payment").unwrap();
        assert_eq!(car.category.as_deref(), Some("Transportation"), "a covered cost budgets under what it covers");
        let gym = r.lines.iter().find(|l| l.label == "Gym").unwrap();
        assert_eq!(gym.category, None, "a cost covering nothing has no category to write to");
        let dining = r.lines.iter().find(|l| l.label == "Dining").unwrap();
        assert_eq!(dining.category.as_deref(), Some("Dining"));
        // Savings lines are labels, not spending categories.
        assert!(r.lines.iter().filter(|l| l.bucket == Bucket::Savings).all(|l| l.category.is_none()));
    }

    #[test]
    fn an_uncovered_fixed_cost_is_added_on_top() {
        let r = project(&input(vec![fixed("Gym", 5_000, Bucket::Wants, None)])).unwrap();
        // Wants still suggests its usual categories, sized to what's left.
        assert!(r.lines.iter().any(|l| l.label == "Dining"));
        assert!(r.lines.iter().any(|l| l.label == "Gym"));
        assert_eq!(r.buckets[1].remaining_cents, 180_000 - 5_000);
    }

    #[test]
    fn fixed_costs_over_the_bucket_are_flagged() {
        // $3,500 of rent against a $3,000 needs budget.
        let r = project(&input(vec![fixed("Rent", 350_000, Bucket::Needs, Some("Housing"))])).unwrap();
        assert!(r.over_allocated);
        assert_eq!(r.buckets[0].remaining_cents, -50_000);
        // Nothing else gets suggested in that bucket; it's already spent.
        assert!(!r.lines.iter().any(|l| l.bucket == Bucket::Needs && l.source == LineSource::Suggested));
    }

    #[test]
    fn validation() {
        let mut i = input(vec![]);
        i.gross_annual_income_cents = 0;
        assert!(project(&i).is_err());

        let mut i = input(vec![]);
        i.needs_pct = 60.0; // 60 + 30 + 20 != 100
        assert!(project(&i).is_err());

        let mut i = input(vec![]);
        i.tax_rate_pct = 120.0;
        assert!(project(&i).is_err());

        let i = input(vec![fixed("Oops", -1, Bucket::Needs, None)]);
        assert!(project(&i).is_err());
    }
}
