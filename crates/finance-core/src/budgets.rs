//! Monthly spending limits per category.
//!
//! Each month stands on its own: nothing rolls over. Spending here matches
//! the rest of the app — transfers between your own accounts don't count,
//! and refunds reduce what you've spent.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::accounts::Scope;
use crate::categories;
use crate::error::{CoreError, Result};

/// A category is "close" to its limit at 85%.
const CLOSE_AT: f64 = 0.85;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetState {
    Under,
    Close,
    Over,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetRow {
    pub category_id: i64,
    pub category_name: String,
    /// None when the category has no limit this month.
    pub limit_cents: Option<i64>,
    pub spent_cents: i64,
    /// Limit minus spent; negative when over.
    pub remaining_cents: Option<i64>,
    /// 0.0 to 1.0+ (over 1 means over budget).
    pub used: Option<f64>,
    pub state: Option<BudgetState>,
    /// Average spend in this category over the three months before.
    pub recent_average_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetMonth {
    pub month: String,
    pub rows: Vec<BudgetRow>,
    pub total_limit_cents: i64,
    pub total_spent_cents: i64,
    /// Spending in categories with no limit, plus uncategorized.
    pub unbudgeted_spent_cents: i64,
    pub income_cents: i64,
}

/// Validate `YYYY-MM` and return it normalised.
pub fn parse_month(month: &str) -> Result<String> {
    let m = month.trim();
    let bad = || CoreError::Invalid(format!("'{m}' is not a month (use YYYY-MM)."));
    let (y, mo) = m.split_once('-').ok_or_else(bad)?;
    let (y, mo): (i32, u32) = (y.parse().map_err(|_| bad())?, mo.parse().map_err(|_| bad())?);
    if !(1900..=2200).contains(&y) || !(1..=12).contains(&mo) {
        return Err(bad());
    }
    Ok(format!("{y:04}-{mo:02}"))
}

/// First and last day of a `YYYY-MM` month.
pub fn month_range(month: &str) -> Result<(String, String)> {
    let month = parse_month(month)?;
    let (y, mo): (i32, u32) = (month[..4].parse().unwrap(), month[5..].parse().unwrap());
    let first = NaiveDate::from_ymd_opt(y, mo, 1).ok_or_else(|| CoreError::Invalid("Bad month.".into()))?;
    let last = first
        .checked_add_months(chrono::Months::new(1))
        .and_then(|d| d.pred_opt())
        .ok_or_else(|| CoreError::Invalid("Bad month.".into()))?;
    Ok((first.format("%Y-%m-%d").to_string(), last.format("%Y-%m-%d").to_string()))
}

pub fn shift_month(month: &str, months: i32) -> Result<String> {
    let month = parse_month(month)?;
    let (y, mo): (i32, u32) = (month[..4].parse().unwrap(), month[5..].parse().unwrap());
    let total = y * 12 + (mo as i32 - 1) + months;
    Ok(format!("{:04}-{:02}", total.div_euclid(12), total.rem_euclid(12) + 1))
}

pub fn month_of(date: &str) -> Result<String> {
    let d = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("'{date}' is not a date.")))?;
    Ok(format!("{:04}-{:02}", d.year(), d.month()))
}

/// Spending per category in a month, using the same rules as the monthly
/// summary: transfers excluded, refunds subtracted.
fn spend_by_category(conn: &Connection, month: &str, scope: Scope) -> Result<HashMap<i64, i64>> {
    let (from, to) = month_range(month)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT coalesce(t.category_id, 0),
                coalesce(sum(CASE WHEN t.amount_cents < 0 AND coalesce(c.kind,'expense') = 'expense'
                                  THEN -t.amount_cents
                                  WHEN t.amount_cents > 0 AND c.kind = 'expense'
                                  THEN -t.amount_cents END), 0)
         FROM transactions t
         JOIN accounts a ON a.id = t.account_id
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE t.posted_on BETWEEN ?1 AND ?2 AND t.transfer_account_id IS NULL AND {}
         GROUP BY t.category_id",
        scope.sql()
    ))?;
    let rows = stmt.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn income_for(conn: &Connection, month: &str, scope: Scope) -> Result<i64> {
    let (from, to) = month_range(month)?;
    Ok(conn.query_row(
        &format!(
            "SELECT coalesce(sum(CASE WHEN t.amount_cents > 0 AND c.kind = 'income'
                                      THEN t.amount_cents END), 0)
             FROM transactions t
             JOIN accounts a ON a.id = t.account_id
             LEFT JOIN categories c ON c.id = t.category_id
             WHERE t.posted_on BETWEEN ?1 AND ?2 AND t.transfer_account_id IS NULL AND {}",
            scope.sql()
        ),
        params![from, to],
        |r| r.get(0),
    )?)
}

fn limits_for(conn: &Connection, month: &str) -> Result<HashMap<i64, i64>> {
    let mut stmt = conn.prepare("SELECT category_id, limit_cents FROM budgets WHERE month = ?1")?;
    let rows = stmt.query_map([month], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Average monthly spend per category over the three months before `month`.
pub fn recent_averages(conn: &Connection, month: &str, scope: Scope) -> Result<HashMap<i64, i64>> {
    let mut totals: HashMap<i64, i64> = HashMap::new();
    for back in 1..=3 {
        for (cat, spent) in spend_by_category(conn, &shift_month(month, -back)?, scope)? {
            *totals.entry(cat).or_default() += spent;
        }
    }
    Ok(totals.into_iter().map(|(k, v)| (k, v / 3)).collect())
}

pub fn month_view(conn: &Connection, month: &str, scope: Scope) -> Result<BudgetMonth> {
    let month = parse_month(month)?;
    let spend = spend_by_category(conn, &month, scope)?;
    let limits = limits_for(conn, &month)?;
    let averages = recent_averages(conn, &month, scope)?;

    let mut rows = Vec::new();
    let mut total_limit = 0;
    let mut total_spent = 0;
    let mut unbudgeted = spend.get(&0).copied().unwrap_or(0); // uncategorized
    for c in categories::list(conn)? {
        if c.kind != "expense" {
            continue;
        }
        let spent = spend.get(&c.id).copied().unwrap_or(0);
        let limit = limits.get(&c.id).copied();
        let average = averages.get(&c.id).copied().unwrap_or(0);
        total_spent += spent;
        match limit {
            Some(l) => total_limit += l,
            None => unbudgeted += spent,
        }
        if limit.is_none() && spent == 0 && average == 0 {
            continue; // nothing to show for this category
        }
        let used = limit.filter(|l| *l > 0).map(|l| spent as f64 / l as f64);
        rows.push(BudgetRow {
            category_id: c.id,
            category_name: c.name,
            limit_cents: limit,
            spent_cents: spent,
            remaining_cents: limit.map(|l| l - spent),
            used,
            state: used.map(|u| {
                if u > 1.0 {
                    BudgetState::Over
                } else if u >= CLOSE_AT {
                    BudgetState::Close
                } else {
                    BudgetState::Under
                }
            }),
            recent_average_cents: average,
        });
    }
    // Budgeted categories first, then the biggest spenders.
    rows.sort_by(|a, b| {
        b.limit_cents
            .is_some()
            .cmp(&a.limit_cents.is_some())
            .then(b.spent_cents.cmp(&a.spent_cents))
            .then(a.category_name.to_lowercase().cmp(&b.category_name.to_lowercase()))
    });

    Ok(BudgetMonth {
        month: month.clone(),
        rows,
        total_limit_cents: total_limit,
        total_spent_cents: total_spent,
        unbudgeted_spent_cents: unbudgeted,
        income_cents: income_for(conn, &month, scope)?,
    })
}

/// Set a category's limit for a month. A limit of 0 removes it.
pub fn set_limit(conn: &Connection, category_id: i64, month: &str, limit_cents: i64) -> Result<()> {
    let month = parse_month(month)?;
    if !(0..=crate::transactions::MAX_AMOUNT_CENTS).contains(&limit_cents) {
        return Err(CoreError::Invalid("That limit doesn't look right.".into()));
    }
    let exists: bool =
        conn.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)", [category_id], |r| r.get(0))?;
    if !exists {
        return Err(CoreError::Invalid("Category not found.".into()));
    }
    if limit_cents == 0 {
        conn.execute(
            "DELETE FROM budgets WHERE category_id = ?1 AND month = ?2",
            params![category_id, month],
        )?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO budgets (category_id, month, limit_cents) VALUES (?1, ?2, ?3)
         ON CONFLICT(category_id, month) DO UPDATE SET limit_cents = excluded.limit_cents",
        params![category_id, month, limit_cents],
    )?;
    Ok(())
}

// ------------------------------------------------------- building a budget

/// How many months back the draft looks for spending patterns.
const LOOKBACK_MONTHS: i32 = 3;

/// Income is looked at over a longer window than spending: pay arrives in
/// lumps (a paycheck landing on the 31st lands in the next month's totals),
/// so a longer run of months makes a steadier figure.
const INCOME_LOOKBACK_MONTHS: i32 = 6;

/// What a month's income looks like against what you usually bring in. A
/// savings rate measured against a month that hasn't been paid yet is
/// meaningless, so this gives something sensible to measure against.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeBaseline {
    pub month: String,
    /// Average over the previous months that had any income at all.
    pub typical_monthly_cents: i64,
    /// How many of those months there were.
    pub months_of_data: u32,
    /// Income actually recorded in `month` so far.
    pub received_cents: i64,
    /// What to measure against: what's been received, or what you usually
    /// bring in when that's more (a month that hasn't been paid yet).
    pub expected_cents: i64,
    /// True when `expected_cents` is the typical figure rather than what has
    /// actually landed — so the UI can say it's an estimate.
    pub estimated: bool,
}

/// Average monthly income before `month`, and what this month should be
/// measured against.
pub fn income_baseline(conn: &Connection, month: &str, scope: Scope) -> Result<IncomeBaseline> {
    let month = parse_month(month)?;
    let mut total = 0;
    let mut months_of_data = 0;
    for back in 1..=INCOME_LOOKBACK_MONTHS {
        let income = income_for(conn, &shift_month(&month, -back)?, scope)?;
        if income > 0 {
            total += income;
            months_of_data += 1;
        }
    }
    let typical_monthly_cents = if months_of_data > 0 { total / months_of_data as i64 } else { 0 };
    let received_cents = income_for(conn, &month, scope)?;
    // Only stand in for what's been received when there's history to stand
    // on and this month is genuinely behind it.
    let estimated = months_of_data > 0 && received_cents < typical_monthly_cents;
    Ok(IncomeBaseline {
        month,
        typical_monthly_cents,
        months_of_data,
        received_cents,
        expected_cents: if estimated { typical_monthly_cents } else { received_cents },
        estimated,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FixedPayment {
    pub account_id: i64,
    pub account_name: String,
    pub monthly_payment_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanLine {
    pub category_id: i64,
    pub category_name: String,
    /// What to budget: the recent average, rounded up.
    pub suggested_cents: i64,
    pub recent_average_cents: i64,
    /// What's already set for this month, if anything.
    pub current_limit_cents: Option<i64>,
}

/// A starting point for a month's budget, derived from what actually
/// happened in the months before it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetDraft {
    pub month: String,
    /// Prior months that had any activity, out of `LOOKBACK_MONTHS`.
    pub months_of_data: u32,
    /// Average monthly income across the months that had any.
    pub monthly_income_cents: i64,
    /// Average monthly amount tagged as moved into savings or investments.
    pub saving_investing_cents: i64,
    /// Usual payments on debts you track, taken from the accounts themselves.
    pub debt_payments: Vec<FixedPayment>,
    pub debt_payments_cents: i64,
    pub lines: Vec<PlanLine>,
    pub suggested_total_cents: i64,
    /// Income minus savings, debt payments and the suggested limits.
    /// Negative means the plan spends more than comes in.
    pub leftover_cents: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitInput {
    pub category_id: i64,
    pub limit_cents: i64,
}

/// Round up to the nearest $5 once the amount is worth rounding, so limits
/// read like decisions rather than averages.
fn round_up(cents: i64) -> i64 {
    let step = if cents >= 2_000 { 500 } else { 100 };
    (cents + step - 1) / step * step
}

/// Money tagged as moving into a savings or investment account in one month.
fn saved_in(conn: &Connection, month: &str, scope: Scope) -> Result<i64> {
    let (from, to) = month_range(month)?;
    crate::transactions::saved_between(conn, &from, &to, scope)
}

/// Whether a month had any transactions at all, so empty months don't drag
/// the averages down.
fn has_activity(conn: &Connection, month: &str, scope: Scope) -> Result<bool> {
    let (from, to) = month_range(month)?;
    Ok(conn.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM transactions t JOIN accounts a ON a.id = t.account_id
                           WHERE t.posted_on BETWEEN ?1 AND ?2 AND {})",
            scope.sql()
        ),
        params![from, to],
        |r| r.get(0),
    )?)
}

/// Usual payments on debts you track, from `accounts.monthly_payment_cents`.
fn debt_payments(conn: &Connection, scope: Scope) -> Result<Vec<FixedPayment>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT a.id, a.name, a.monthly_payment_cents FROM accounts a
         WHERE a.archived = 0 AND a.is_asset = 0 AND coalesce(a.monthly_payment_cents, 0) > 0 AND {}
         ORDER BY a.monthly_payment_cents DESC",
        scope.sql()
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok(FixedPayment { account_id: r.get(0)?, account_name: r.get(1)?, monthly_payment_cents: r.get(2)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Build a suggested budget for `month` from the previous months: income and
/// saving derived from the accounts, per-category limits from what was
/// actually spent, debt payments read off the debt accounts.
pub fn draft(conn: &Connection, month: &str, scope: Scope) -> Result<BudgetDraft> {
    let month = parse_month(month)?;

    let mut saved_total = 0;
    let mut months_of_data = 0;
    for back in 1..=LOOKBACK_MONTHS {
        let m = shift_month(&month, -back)?;
        if !has_activity(conn, &m, scope)? {
            continue;
        }
        months_of_data += 1;
        saved_total += saved_in(conn, &m, scope)?;
    }
    // Income over a longer window than spending, for the same reason the
    // dashboard does: pay arrives in lumps.
    let monthly_income_cents = income_baseline(conn, &month, scope)?.typical_monthly_cents;
    let saving_investing_cents = if months_of_data > 0 { saved_total / months_of_data as i64 } else { 0 };

    let averages = recent_averages(conn, &month, scope)?;
    let limits = limits_for(conn, &month)?;
    let mut lines: Vec<PlanLine> = categories::list(conn)?
        .into_iter()
        .filter(|c| c.kind == "expense")
        .filter_map(|c| {
            let average = averages.get(&c.id).copied().unwrap_or(0);
            let current = limits.get(&c.id).copied();
            if average == 0 && current.is_none() {
                return None;
            }
            Some(PlanLine {
                category_id: c.id,
                category_name: c.name,
                suggested_cents: if average > 0 { round_up(average) } else { current.unwrap_or(0) },
                recent_average_cents: average,
                current_limit_cents: current,
            })
        })
        .collect();
    lines.sort_by(|a, b| {
        b.suggested_cents
            .cmp(&a.suggested_cents)
            .then(a.category_name.to_lowercase().cmp(&b.category_name.to_lowercase()))
    });

    let suggested_total_cents: i64 = lines.iter().map(|l| l.suggested_cents).sum();
    let payments = debt_payments(conn, scope)?;
    let debt_payments_cents: i64 = payments.iter().map(|p| p.monthly_payment_cents).sum();

    Ok(BudgetDraft {
        month,
        months_of_data,
        monthly_income_cents,
        saving_investing_cents,
        debt_payments: payments,
        debt_payments_cents,
        leftover_cents: monthly_income_cents
            - saving_investing_cents
            - debt_payments_cents
            - suggested_total_cents,
        lines,
        suggested_total_cents,
    })
}

/// Apply a whole set of limits to a month at once. A limit of 0 clears that
/// category, same as `set_limit`.
pub fn set_limits(conn: &mut Connection, month: &str, limits: &[LimitInput]) -> Result<usize> {
    let month = parse_month(month)?;
    let tx = conn.transaction()?;
    for l in limits {
        set_limit(&tx, l.category_id, &month, l.limit_cents)?;
    }
    tx.commit()?;
    Ok(limits.len())
}

/// Copy every limit from one month to another, replacing what's there.
pub fn copy_month(conn: &mut Connection, from: &str, to: &str) -> Result<usize> {
    let (from, to) = (parse_month(from)?, parse_month(to)?);
    if from == to {
        return Err(CoreError::Invalid("Pick a different month to copy from.".into()));
    }
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM budgets WHERE month = ?1", [&to])?;
    let n = tx.execute(
        "INSERT INTO budgets (category_id, month, limit_cents)
         SELECT category_id, ?2, limit_cents FROM budgets WHERE month = ?1",
        params![from, to],
    )?;
    tx.commit()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{self, tests::{conn, input}};
    use crate::transactions::{self, tests::tx};

    fn cat(c: &Connection, name: &str) -> i64 {
        categories::find_by_name(c, name).unwrap().unwrap()
    }

    fn spend(c: &Connection, account: i64, date: &str, cents: i64, category: &str) {
        let mut t = tx(account, date, cents, "Shop");
        t.category_id = Some(cat(c, category));
        transactions::create(c, &t).unwrap();
    }

    #[test]
    fn months() {
        assert_eq!(parse_month(" 2026-9 ").unwrap(), "2026-09");
        assert!(parse_month("2026-13").is_err());
        assert!(parse_month("September").is_err());
        assert_eq!(month_range("2026-02").unwrap(), ("2026-02-01".into(), "2026-02-28".into()));
        assert_eq!(month_range("2024-02").unwrap().1, "2024-02-29");
        assert_eq!(shift_month("2026-01", -1).unwrap(), "2025-12");
        assert_eq!(shift_month("2026-12", 1).unwrap(), "2027-01");
        assert_eq!(month_of("2026-09-17").unwrap(), "2026-09");
    }

    #[test]
    fn spending_limits_and_states() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let groceries = cat(&c, "Groceries");
        set_limit(&c, groceries, "2026-09", 50_000).unwrap();
        set_limit(&c, cat(&c, "Dining"), "2026-09", 20_000).unwrap();
        set_limit(&c, cat(&c, "Travel"), "2026-09", 10_000).unwrap();

        spend(&c, a.id, "2026-09-02", -30_000, "Groceries");
        spend(&c, a.id, "2026-09-09", -5_000, "Groceries"); // 70% -> under
        spend(&c, a.id, "2026-09-10", -18_000, "Dining"); // 90% -> close
        spend(&c, a.id, "2026-09-11", -12_000, "Travel"); // 120% -> over
        spend(&c, a.id, "2026-09-12", -4_000, "Shopping"); // no limit
        // A refund reduces spending, a transfer never counts.
        spend(&c, a.id, "2026-09-13", 2_000, "Groceries");
        let mut moved = tx(a.id, "2026-09-14", -100_000, "To savings");
        moved.transfer_account_id = Some(accounts::create(&mut c, &input("HYSA", "hysa")).unwrap().id);
        transactions::create(&c, &moved).unwrap();
        // Next month doesn't affect this one.
        spend(&c, a.id, "2026-10-01", -90_000, "Groceries");

        let v = month_view(&c, "2026-09", Scope::Real).unwrap();
        let row = |name: &str| v.rows.iter().find(|r| r.category_name == name).unwrap();
        assert_eq!(row("Groceries").spent_cents, 33_000);
        assert_eq!(row("Groceries").remaining_cents, Some(17_000));
        assert_eq!(row("Groceries").state, Some(BudgetState::Under));
        assert_eq!(row("Dining").state, Some(BudgetState::Close));
        assert_eq!(row("Travel").state, Some(BudgetState::Over));
        assert_eq!(row("Travel").remaining_cents, Some(-2_000));
        assert_eq!(row("Shopping").limit_cents, None);
        assert_eq!(v.total_limit_cents, 80_000);
        assert_eq!(v.total_spent_cents, 33_000 + 18_000 + 12_000 + 4_000);
        assert_eq!(v.unbudgeted_spent_cents, 4_000);
        // Categories with no limit and no spending stay out of the way.
        assert!(!v.rows.iter().any(|r| r.category_name == "Insurance"));
    }

    #[test]
    fn uncategorized_counts_as_unbudgeted_and_income_shows() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        transactions::create(&c, &tx(a.id, "2026-09-03", -7_500, "Mystery")).unwrap();
        spend(&c, a.id, "2026-09-04", 400_000, "Income");
        let v = month_view(&c, "2026-09", Scope::Real).unwrap();
        assert_eq!(v.unbudgeted_spent_cents, 7_500);
        assert_eq!(v.income_cents, 400_000);
        assert_eq!(v.total_spent_cents, 0, "uncategorized isn't in a category row");
    }

    #[test]
    fn averages_and_copying_a_month() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        spend(&c, a.id, "2026-06-05", -30_000, "Groceries");
        spend(&c, a.id, "2026-07-05", -60_000, "Groceries");
        spend(&c, a.id, "2026-08-05", -30_000, "Groceries");
        let v = month_view(&c, "2026-09", Scope::Real).unwrap();
        let g = v.rows.iter().find(|r| r.category_name == "Groceries").unwrap();
        assert_eq!(g.recent_average_cents, 40_000);
        assert_eq!(g.spent_cents, 0);

        set_limit(&c, cat(&c, "Groceries"), "2026-09", 45_000).unwrap();
        set_limit(&c, cat(&c, "Dining"), "2026-09", 15_000).unwrap();
        assert_eq!(copy_month(&mut c, "2026-09", "2026-10").unwrap(), 2);
        let oct = month_view(&c, "2026-10", Scope::Real).unwrap();
        assert_eq!(oct.total_limit_cents, 60_000);
        // Copying again replaces rather than duplicating.
        set_limit(&c, cat(&c, "Groceries"), "2026-10", 99_000).unwrap();
        copy_month(&mut c, "2026-09", "2026-10").unwrap();
        assert_eq!(month_view(&c, "2026-10", Scope::Real).unwrap().total_limit_cents, 60_000);
        assert!(copy_month(&mut c, "2026-10", "2026-10").is_err());

        // Setting 0 removes the limit.
        set_limit(&c, cat(&c, "Dining"), "2026-10", 0).unwrap();
        assert_eq!(month_view(&c, "2026-10", Scope::Real).unwrap().total_limit_cents, 45_000);
        assert!(set_limit(&c, 9_999, "2026-10", 1_000).is_err());
        assert!(set_limit(&c, cat(&c, "Dining"), "2026-10", -5).is_err());
    }

    #[test]
    fn draft_derives_income_saving_and_limits_from_history() {
        let mut c = conn();
        let checking = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let hysa = accounts::create(&mut c, &input("Ally HYSA", "hysa")).unwrap();
        let brokerage = accounts::create(&mut c, &input("Brokerage", "brokerage")).unwrap();
        let mut loan = input("Car loan", "loan");
        loan.monthly_payment_cents = Some(45_000);
        accounts::create(&mut c, &loan).unwrap();

        // Three months of history before September.
        for month in ["2026-06", "2026-07", "2026-08"] {
            let mut pay = tx(checking.id, &format!("{month}-01"), 500_000, "Payroll");
            pay.category_id = Some(cat(&c, "Income"));
            transactions::create(&c, &pay).unwrap();
            spend(&c, checking.id, &format!("{month}-05"), -60_000, "Groceries");
            spend(&c, checking.id, &format!("{month}-06"), -12_300, "Dining");
            // Saving and investing: tagged transfers into the HYSA and brokerage.
            for (target, cents) in [(hysa.id, -100_000), (brokerage.id, -50_000)] {
                let mut moved = tx(checking.id, &format!("{month}-07"), cents, "Transfer");
                moved.transfer_account_id = Some(target);
                transactions::create(&c, &moved).unwrap();
            }
        }

        let d = draft(&c, "2026-09", Scope::Real).unwrap();
        assert_eq!(d.months_of_data, 3);
        assert_eq!(d.monthly_income_cents, 500_000);
        assert_eq!(d.saving_investing_cents, 150_000, "both transfers count, averaged per month");
        assert_eq!(d.debt_payments_cents, 45_000);
        assert_eq!(d.debt_payments[0].account_name, "Car loan");

        let groceries = d.lines.iter().find(|l| l.category_name == "Groceries").unwrap();
        assert_eq!(groceries.recent_average_cents, 60_000);
        assert_eq!(groceries.suggested_cents, 60_000);
        // Averages are rounded up to a readable figure.
        let dining = d.lines.iter().find(|l| l.category_name == "Dining").unwrap();
        assert_eq!(dining.recent_average_cents, 12_300);
        assert_eq!(dining.suggested_cents, 12_500);

        // Income minus saving, debt payments and the suggested limits.
        assert_eq!(d.leftover_cents, 500_000 - 150_000 - 45_000 - d.suggested_total_cents);

        // Applying the plan writes the limits for that month.
        let limits: Vec<LimitInput> = d
            .lines
            .iter()
            .map(|l| LimitInput { category_id: l.category_id, limit_cents: l.suggested_cents })
            .collect();
        set_limits(&mut c, "2026-09", &limits).unwrap();
        let view = month_view(&c, "2026-09", Scope::Real).unwrap();
        assert_eq!(view.total_limit_cents, d.suggested_total_cents);
    }

    #[test]
    fn income_baseline_stands_in_for_a_month_that_hasnt_been_paid_yet() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let pay = |date: &str, cents: i64| {
            let mut t = tx(a.id, date, cents, "Payroll");
            t.category_id = Some(cat(&c, "Income"));
            transactions::create(&c, &t).unwrap();
        };
        // Paid on the last day of each month, so October's calendar month
        // shows nothing even though September 30th's pay covers it.
        pay("2026-07-31", 400_000);
        pay("2026-08-31", 400_000);
        pay("2026-09-30", 400_000);

        let b = income_baseline(&c, "2026-10", Scope::Real).unwrap();
        assert_eq!(b.received_cents, 0, "nothing has landed in October yet");
        assert_eq!(b.typical_monthly_cents, 400_000);
        assert_eq!(b.months_of_data, 3);
        assert!(b.estimated);
        assert_eq!(b.expected_cents, 400_000, "measure against what you usually bring in");

        // A month that has been paid uses the real figure, not the average.
        let b = income_baseline(&c, "2026-09", Scope::Real).unwrap();
        assert_eq!(b.received_cents, 400_000);
        assert!(!b.estimated);
        assert_eq!(b.expected_cents, 400_000);

        // A better-than-usual month isn't talked down to the average.
        pay("2026-10-15", 900_000);
        let b = income_baseline(&c, "2026-10", Scope::Real).unwrap();
        assert_eq!(b.received_cents, 900_000);
        assert!(!b.estimated);
        assert_eq!(b.expected_cents, 900_000);
    }

    #[test]
    fn income_baseline_without_history_doesnt_invent_income() {
        let mut c = conn();
        accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let b = income_baseline(&c, "2026-10", Scope::Real).unwrap();
        assert_eq!((b.typical_monthly_cents, b.months_of_data, b.expected_cents), (0, 0, 0));
        assert!(!b.estimated, "with nothing to average, there's nothing to estimate from");
    }

    #[test]
    fn draft_without_history_is_empty_not_an_error() {
        let mut c = conn();
        accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let d = draft(&c, "2026-09", Scope::Real).unwrap();
        assert_eq!(d.months_of_data, 0);
        assert_eq!(d.monthly_income_cents, 0);
        assert_eq!(d.saving_investing_cents, 0);
        assert!(d.lines.is_empty());
        assert_eq!(d.leftover_cents, 0);
    }

    #[test]
    fn test_accounts_stay_out_of_real_budgets() {
        let mut c = conn();
        let real = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let fake = accounts::create(&mut c, &input("Plaid Checking", "checking")).unwrap();
        accounts::set_is_test(&c, fake.id, true).unwrap();
        spend(&c, real.id, "2026-09-02", -10_000, "Groceries");
        spend(&c, fake.id, "2026-09-02", -99_000, "Groceries");
        assert_eq!(month_view(&c, "2026-09", Scope::Real).unwrap().total_spent_cents, 10_000);
        assert_eq!(month_view(&c, "2026-09", Scope::Test).unwrap().total_spent_cents, 99_000);
    }
}
