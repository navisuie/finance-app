//! Debts you track yourself: payoff projections and the payments you link to
//! them.
//!
//! A debt you track by hand (with an APR set) has its balance kept current
//! from the payments you tag against it: last statement balance, plus
//! estimated interest, minus what you've paid. The statement balance you
//! enter is always the anchor, so correcting it from a real statement resets
//! the running total. A Plaid-fed debt is left alone — the bank's own figure
//! wins there.

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::accounts::{self, Account, Scope};
use crate::error::{CoreError, Result};
use crate::util::today;

/// Stop projecting after 50 years.
const MAX_MONTHS: u32 = 600;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Payoff {
    pub months: u32,
    /// `YYYY-MM-DD` of the last payment.
    pub payoff_on: String,
    pub total_interest_cents: i64,
    pub total_paid_cents: i64,
    pub last_payment_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Projection {
    /// Nothing owed.
    Clear,
    /// Not enough information yet (no APR or no payment amount).
    Incomplete,
    /// The payment doesn't cover the interest, so the balance grows.
    NeverPaidOff { monthly_interest_cents: i64, minimum_to_make_progress_cents: i64 },
    Paid(Payoff),
}

/// Interest for one month on `balance_cents` at `apr_bps` (basis points).
pub(crate) fn monthly_interest(balance_cents: i64, apr_bps: i64) -> i64 {
    if balance_cents <= 0 || apr_bps <= 0 {
        return 0;
    }
    let numerator = balance_cents as i128 * apr_bps as i128;
    // Round to the nearest cent.
    ((numerator + 60_000) / 120_000) as i64
}

pub(crate) fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    date.checked_add_months(chrono::Months::new(months)).unwrap_or(date)
}

/// How long this debt takes to clear at a fixed monthly payment.
pub fn project(balance_cents: i64, apr_bps: Option<i64>, payment_cents: Option<i64>, from: NaiveDate) -> Projection {
    if balance_cents <= 0 {
        return Projection::Clear;
    }
    let (Some(apr), Some(payment)) = (apr_bps, payment_cents) else {
        return Projection::Incomplete;
    };
    if payment <= 0 || apr < 0 {
        return Projection::Incomplete;
    }
    let first_interest = monthly_interest(balance_cents, apr);
    if payment <= first_interest {
        return Projection::NeverPaidOff {
            monthly_interest_cents: first_interest,
            minimum_to_make_progress_cents: first_interest + 1,
        };
    }

    let mut balance = balance_cents;
    let mut total_interest: i64 = 0;
    let mut total_paid: i64 = 0;
    let mut months = 0;
    let mut last_payment = payment;
    while balance > 0 && months < MAX_MONTHS {
        let interest = monthly_interest(balance, apr);
        let due = balance + interest;
        let pay = payment.min(due);
        balance = due - pay;
        total_interest += interest;
        total_paid += pay;
        last_payment = pay;
        months += 1;
    }
    if balance > 0 {
        // Shouldn't happen once the payment beats the interest, but never
        // report a payoff we didn't reach.
        return Projection::NeverPaidOff {
            monthly_interest_cents: first_interest,
            minimum_to_make_progress_cents: first_interest + 1,
        };
    }
    Projection::Paid(Payoff {
        months,
        payoff_on: add_months(from, months).format("%Y-%m-%d").to_string(),
        total_interest_cents: total_interest,
        total_paid_cents: total_paid,
        last_payment_cents: last_payment,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaymentsSince {
    /// The balance date these payments come after.
    pub since: Option<String>,
    pub count: i64,
    pub total_cents: i64,
}

/// Payments linked to this account since its latest balance snapshot.
pub fn payments_since_balance(conn: &Connection, account_id: i64) -> Result<PaymentsSince> {
    let since: Option<String> = conn
        .query_row(
            "SELECT as_of FROM balances WHERE account_id = ?1 ORDER BY as_of DESC, id DESC LIMIT 1",
            [account_id],
            |r| r.get(0),
        )
        .optional()?;
    let (count, total): (i64, i64) = conn.query_row(
        "SELECT count(*), coalesce(sum(-amount_cents), 0) FROM transactions
         WHERE transfer_account_id = ?1 AND amount_cents < 0 AND (?2 IS NULL OR posted_on > ?2)",
        rusqlite::params![account_id, since],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(PaymentsSince { since, count, total_cents: total })
}

// ------------------------------------------------- keeping the balance current

/// The source recorded on balances this calculation writes. Any other source
/// — a statement balance you entered, or one Plaid reported — anchors it.
pub const AUTO_SOURCE: &str = "auto";

/// True when the app maintains this debt's balance itself: a debt you track
/// by hand with an APR set. A Plaid-fed account keeps whatever the bank says.
fn tracks_balance_itself(conn: &Connection, account: &Account) -> Result<bool> {
    if account.is_asset || account.archived || account.linked {
        return Ok(false);
    }
    let apr: Option<i64> =
        conn.query_row("SELECT apr_bps FROM accounts WHERE id = ?1", [account.id], |r| r.get(0))?;
    Ok(apr.is_some())
}

/// The most recent balance that didn't come from this calculation.
fn anchor(conn: &Connection, account_id: i64) -> Result<Option<(String, i64)>> {
    Ok(conn
        .query_row(
            "SELECT as_of, amount_cents FROM balances
             WHERE account_id = ?1 AND source != ?2 ORDER BY as_of DESC, id DESC LIMIT 1",
            rusqlite::params![account_id, AUTO_SOURCE],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

/// Interest we estimate has accrued on `balance` over `days` at `apr_bps`.
fn accrued_interest(balance_cents: i64, apr_bps: Option<i64>, days: i64) -> i64 {
    match apr_bps {
        Some(apr) if apr > 0 && balance_cents > 0 && days > 0 => {
            ((balance_cents as i128 * apr as i128 * days as i128) / (10_000 * 365)) as i64
        }
        _ => 0,
    }
}

/// Bring this debt's balance up to date from the payments tagged against it:
/// the last statement balance, plus estimated interest since, minus what
/// you've paid. Recomputed from that anchor every time, so calling it twice
/// never double-counts. Returns the new balance when it wrote one.
///
/// Only writes once there's at least one payment to apply, so a debt never
/// grows on its own behind your back.
pub fn refresh_auto_balance(conn: &Connection, account_id: i64) -> Result<Option<i64>> {
    let account = accounts::get(conn, account_id)?;
    if !tracks_balance_itself(conn, &account)? {
        return Ok(None);
    }
    // Clear the snapshot from the last run first, so this recomputes from
    // scratch and a statement balance dated in the past still wins.
    conn.execute(
        "DELETE FROM balances WHERE account_id = ?1 AND source = ?2",
        rusqlite::params![account_id, AUTO_SOURCE],
    )?;
    let Some((anchored_on, anchored_cents)) = anchor(conn, account_id)? else {
        return Ok(None); // nothing to count from yet
    };
    let (paid_count, paid_cents): (i64, i64) = conn.query_row(
        "SELECT count(*), coalesce(sum(-amount_cents), 0) FROM transactions
         WHERE transfer_account_id = ?1 AND amount_cents < 0 AND posted_on > ?2",
        rusqlite::params![account_id, anchored_on],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if paid_count == 0 {
        return Ok(None);
    }
    let apr: Option<i64> =
        conn.query_row("SELECT apr_bps FROM accounts WHERE id = ?1", [account_id], |r| r.get(0))?;
    let day = today();
    let interest = accrued_interest(anchored_cents, apr, days_between(&anchored_on, &day));
    let balance = (anchored_cents + interest - paid_cents).max(0);
    accounts::upsert_balance(conn, account_id, &day, balance, AUTO_SOURCE)?;
    Ok(Some(balance))
}

/// Refresh every debt whose balance the app maintains. Cheap — there are only
/// ever a handful of debt accounts — so callers don't have to work out which
/// debt a payment touched.
pub fn refresh_all_auto_balances(conn: &Connection) -> Result<usize> {
    let mut n = 0;
    for a in accounts::list(conn, false, Scope::All)? {
        if !a.is_asset && refresh_auto_balance(conn, a.id)?.is_some() {
            n += 1;
        }
    }
    Ok(n)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DebtSummary {
    pub account: Account,
    pub apr_bps: Option<i64>,
    pub monthly_payment_cents: Option<i64>,
    pub payments: PaymentsSince,
    /// The payments you've tagged as paying this debt, newest first, with the
    /// account each came from.
    pub recent_payments: Vec<crate::transactions::LinkedPayment>,
    /// Interest we estimate has built up since the balance date.
    pub estimated_interest_cents: i64,
    /// Balance, plus estimated interest, minus payments you've linked.
    pub suggested_balance_cents: Option<i64>,
    /// True when the app keeps this balance up to date from tagged payments.
    pub auto_tracked: bool,
    /// The statement balance the running total is counted from.
    pub anchored_on: Option<String>,
    pub projection: Projection,
}

fn days_between(from: &str, to: &str) -> i64 {
    match (NaiveDate::parse_from_str(from, "%Y-%m-%d"), NaiveDate::parse_from_str(to, "%Y-%m-%d")) {
        (Ok(a), Ok(b)) => (b - a).num_days().max(0),
        _ => 0,
    }
}

pub fn summary(conn: &Connection, account_id: i64) -> Result<DebtSummary> {
    let account = accounts::get(conn, account_id)?;
    if account.is_asset {
        return Err(CoreError::Invalid(format!("{} isn't a debt account.", account.name)));
    }
    let (apr_bps, monthly_payment_cents): (Option<i64>, Option<i64>) = conn.query_row(
        "SELECT apr_bps, monthly_payment_cents FROM accounts WHERE id = ?1",
        [account_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let auto_tracked = tracks_balance_itself(conn, &account)?;
    let payments = payments_since_balance(conn, account_id)?;
    let balance = account.balance_cents.unwrap_or(0);
    let day = today();

    let estimated_interest_cents = match (account.balance_as_of.as_deref(), apr_bps) {
        (Some(as_of), Some(apr)) if apr > 0 && balance > 0 => {
            let days = days_between(as_of, &day);
            ((balance as i128 * apr as i128 * days as i128) / (10_000 * 365)) as i64
        }
        _ => 0,
    };
    let suggested_balance_cents = if payments.count > 0 {
        Some((balance + estimated_interest_cents - payments.total_cents).max(0))
    } else {
        None
    };

    let projection = project(
        balance,
        apr_bps,
        monthly_payment_cents.or((payments.count == 1).then_some(payments.total_cents)),
        NaiveDate::parse_from_str(&day, "%Y-%m-%d").unwrap_or_default(),
    );

    Ok(DebtSummary {
        account,
        apr_bps,
        monthly_payment_cents,
        payments,
        recent_payments: crate::transactions::payments_to(conn, account_id, 6)?,
        auto_tracked,
        anchored_on: anchor(conn, account_id)?.map(|(as_of, _)| as_of),
        estimated_interest_cents,
        suggested_balance_cents,
        projection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::tests::{conn, input};
    use crate::transactions::{self, tests::tx};

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn interest_math() {
        // $10,000 at 6% -> $50.00 for the month.
        assert_eq!(monthly_interest(1_000_000, 600), 5_000);
        assert_eq!(monthly_interest(0, 600), 0);
        assert_eq!(monthly_interest(1_000_000, 0), 0);
    }

    #[test]
    fn car_loan_payoff() {
        // $18,000 at 6.49%, paying $450 a month.
        // The textbook answer is 45.2 payments, so 46 with the last one small.
        let p = project(1_800_000, Some(649), Some(45_000), date("2026-09-17"));
        let Projection::Paid(payoff) = p else { panic!("{p:?}") };
        assert_eq!(payoff.months, 46);
        assert_eq!(payoff.payoff_on, "2030-07-17");
        assert!(payoff.last_payment_cents < 45_000, "the final payment is just the remainder");
        // Total paid is the balance plus the interest, and the last payment
        // is only the remainder.
        assert_eq!(payoff.total_paid_cents, 1_800_000 + payoff.total_interest_cents);
        assert!(payoff.total_interest_cents > 200_000 && payoff.total_interest_cents < 300_000);
        assert!(payoff.last_payment_cents <= 45_000);

        // Paying more clears it sooner and costs less interest.
        let Projection::Paid(faster) = project(1_800_000, Some(649), Some(60_000), date("2026-09-17")) else {
            panic!()
        };
        assert!(faster.months < payoff.months);
        assert!(faster.total_interest_cents < payoff.total_interest_cents);
    }

    #[test]
    fn interest_free_and_edge_cases() {
        let Projection::Paid(p) = project(120_000, Some(0), Some(10_000), date("2026-01-31")) else {
            panic!()
        };
        assert_eq!(p.months, 12);
        assert_eq!(p.total_interest_cents, 0);
        assert_eq!(p.payoff_on, "2027-01-31");

        assert_eq!(project(0, Some(649), Some(45_000), date("2026-09-17")), Projection::Clear);
        assert_eq!(project(-5, Some(649), Some(45_000), date("2026-09-17")), Projection::Clear);
        assert_eq!(project(100_000, None, Some(45_000), date("2026-09-17")), Projection::Incomplete);
        assert_eq!(project(100_000, Some(649), None, date("2026-09-17")), Projection::Incomplete);
        assert_eq!(project(100_000, Some(649), Some(0), date("2026-09-17")), Projection::Incomplete);
    }

    #[test]
    fn payment_that_never_clears_the_debt() {
        // $10,000 at 24%: interest alone is $200 a month.
        match project(1_000_000, Some(2400), Some(20_000), date("2026-09-17")) {
            Projection::NeverPaidOff { monthly_interest_cents, minimum_to_make_progress_cents } => {
                assert_eq!(monthly_interest_cents, 20_000);
                assert_eq!(minimum_to_make_progress_cents, 20_001);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            project(1_000_000, Some(2400), Some(20_001), date("2026-09-17")),
            Projection::Paid(_)
        ));
    }

    #[test]
    fn tagged_payments_bring_the_balance_down() {
        let mut c = conn();
        let checking = crate::accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let mut loan = input("Car loan", "loan");
        loan.balance_cents = Some(1_800_000); // $18,000 owed
        loan.balance_as_of = Some("2026-08-01".into());
        loan.apr_bps = Some(649);
        let loan = crate::accounts::create(&mut c, &loan).unwrap();

        // Nothing to apply yet, so nothing is written.
        assert_eq!(refresh_auto_balance(&c, loan.id).unwrap(), None);
        assert_eq!(accounts::get(&c, loan.id).unwrap().balance_cents, Some(1_800_000));

        let mut pay = tx(checking.id, "2026-08-05", -70_000, "Honda Financial");
        pay.transfer_account_id = Some(loan.id);
        transactions::create(&c, &pay).unwrap();

        let after_one = refresh_auto_balance(&c, loan.id).unwrap().expect("a payment to apply");
        assert!(after_one < 1_800_000, "the $700 payment brings it down");
        assert_eq!(accounts::get(&c, loan.id).unwrap().balance_cents, Some(after_one));

        // Running again doesn't double-count: it recomputes from the anchor.
        assert_eq!(refresh_auto_balance(&c, loan.id).unwrap(), Some(after_one));
        assert_eq!(accounts::get(&c, loan.id).unwrap().balance_cents, Some(after_one));

        // A second payment takes it down again, by about another $700.
        let mut pay2 = tx(checking.id, "2026-09-05", -70_000, "Honda Financial");
        pay2.transfer_account_id = Some(loan.id);
        transactions::create(&c, &pay2).unwrap();
        let after_two = refresh_auto_balance(&c, loan.id).unwrap().unwrap();
        assert!(after_two < after_one);
        assert!((after_one - after_two - 70_000i64).abs() < 1_000, "roughly the payment, less interest");

        // The statement balance you enter wins and re-anchors the running total.
        accounts::set_balance(&c, loan.id, "2026-09-20", 1_700_000).unwrap();
        assert_eq!(refresh_auto_balance(&c, loan.id).unwrap(), None, "no payments after that statement");
        assert_eq!(accounts::get(&c, loan.id).unwrap().balance_cents, Some(1_700_000));

        let s = summary(&c, loan.id).unwrap();
        assert!(s.auto_tracked);
        assert_eq!(s.anchored_on.as_deref(), Some("2026-09-20"));
    }

    #[test]
    fn a_debt_without_an_apr_or_a_bank_feed_is_left_alone() {
        let mut c = conn();
        let checking = crate::accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        // No APR: we can't estimate interest, so we don't touch the balance.
        let mut loan = input("Family loan", "loan");
        loan.balance_cents = Some(500_000);
        loan.balance_as_of = Some("2026-08-01".into());
        let loan = crate::accounts::create(&mut c, &loan).unwrap();
        let mut pay = tx(checking.id, "2026-08-05", -50_000, "Repayment");
        pay.transfer_account_id = Some(loan.id);
        transactions::create(&c, &pay).unwrap();

        assert_eq!(refresh_auto_balance(&c, loan.id).unwrap(), None);
        assert_eq!(accounts::get(&c, loan.id).unwrap().balance_cents, Some(500_000));
        assert!(!summary(&c, loan.id).unwrap().auto_tracked);

        // An asset account is never treated as a debt.
        let savings = crate::accounts::create(&mut c, &input("Savings", "hysa")).unwrap();
        assert_eq!(refresh_auto_balance(&c, savings.id).unwrap(), None);
    }

    #[test]
    fn linked_payments_are_counted_and_suggest_a_new_balance() {
        let mut c = conn();
        let checking = crate::accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let mut loan = input("Car loan", "loan");
        loan.balance_cents = Some(1_800_000);
        loan.balance_as_of = Some("2026-08-01".into());
        loan.apr_bps = Some(649);
        loan.monthly_payment_cents = Some(45_000);
        let loan = crate::accounts::create(&mut c, &loan).unwrap();
        assert!(!loan.is_asset);

        // A payment before the balance date doesn't count; two after do.
        for (date, amount, linked) in [
            ("2026-07-15", -45_000, true),
            ("2026-08-15", -45_000, true),
            ("2026-09-15", -45_000, true),
            ("2026-09-16", -2_000, false),
        ] {
            let mut t = tx(checking.id, date, amount, "Car payment");
            t.transfer_account_id = if linked { Some(loan.id) } else { None };
            transactions::create(&c, &t).unwrap();
        }

        let p = payments_since_balance(&c, loan.id).unwrap();
        assert_eq!((p.count, p.total_cents), (2, 90_000));
        assert_eq!(p.since.as_deref(), Some("2026-08-01"));

        let s = summary(&c, loan.id).unwrap();
        assert_eq!(s.payments.total_cents, 90_000);
        let suggested = s.suggested_balance_cents.unwrap();
        assert!(suggested < 1_800_000, "payments bring the balance down");
        assert!(suggested > 1_800_000 - 90_000, "estimated interest is added back");
        assert!(s.estimated_interest_cents > 0);
        assert!(matches!(s.projection, Projection::Paid(_)));

        // The balance itself is untouched until the user applies it.
        assert_eq!(crate::accounts::get(&c, loan.id).unwrap().balance_cents, Some(1_800_000));
        assert!(summary(&c, checking.id).is_err(), "assets aren't debts");
    }
}
