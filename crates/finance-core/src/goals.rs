//! Savings goals: a target amount by a date, tracked by an account's balance.

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::accounts::{self, Scope};
use crate::error::{CoreError, Result};
use crate::util::{now_rfc3339, optional_text, parse_date, required_text, today};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub id: i64,
    pub name: String,
    pub target_cents: i64,
    pub target_date: Option<String>,
    pub account_id: Option<i64>,
    pub account_name: Option<String>,
    /// The linked account's latest balance.
    pub current_cents: i64,
    pub remaining_cents: i64,
    /// 0.0 to 1.0+ .
    pub progress: f64,
    pub reached: bool,
    /// Whole months from today to the target date (0 when it's this month).
    pub months_left: Option<i64>,
    /// What to put aside each month to arrive on time.
    pub monthly_needed_cents: Option<i64>,
    /// True when the target date has passed and the goal isn't met.
    pub overdue: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalInput {
    pub name: String,
    pub target_cents: i64,
    pub target_date: Option<String>,
    pub account_id: Option<i64>,
}

struct Clean {
    name: String,
    target_date: Option<String>,
}

fn validate(conn: &Connection, input: &GoalInput) -> Result<Clean> {
    let name = required_text("Goal name", &input.name, 80)?;
    if input.target_cents <= 0 || input.target_cents > crate::transactions::MAX_AMOUNT_CENTS {
        return Err(CoreError::Invalid("Enter the amount you're aiming for.".into()));
    }
    let target_date = match optional_text("Target date", input.target_date.as_deref(), 10)? {
        Some(d) => Some(parse_date(&d)?),
        None => None,
    };
    if let Some(id) = input.account_id {
        let account = accounts::get(conn, id)?;
        if !account.is_asset {
            return Err(CoreError::Invalid(format!(
                "{} is a debt, so it can't hold a savings goal.",
                account.name
            )));
        }
    }
    Ok(Clean { name, target_date })
}

fn months_between(from: &str, to: &str) -> Option<i64> {
    let (a, b) = (
        NaiveDate::parse_from_str(from, "%Y-%m-%d").ok()?,
        NaiveDate::parse_from_str(to, "%Y-%m-%d").ok()?,
    );
    let months = (b.year() as i64 - a.year() as i64) * 12 + (b.month() as i64 - a.month() as i64);
    // Part of a month still counts as a month to save in.
    Some(if b.day() > a.day() { months + 1 } else { months })
}

/// The stored columns of a goal, before progress is worked out.
struct Row {
    id: i64,
    name: String,
    target_cents: i64,
    target_date: Option<String>,
    account_id: Option<i64>,
    account_name: Option<String>,
    current_cents: i64,
}

fn build(row: Row, day: &str) -> Goal {
    let Row { id, name, target_cents, target_date, account_id, account_name, current_cents } = row;
    let remaining = (target_cents - current_cents).max(0);
    let reached = current_cents >= target_cents;
    let months_left = target_date.as_deref().and_then(|d| months_between(day, d));
    let overdue = !reached && months_left.is_some_and(|m| m < 0);
    let monthly_needed_cents = match (reached, months_left) {
        (true, _) => None,
        (false, Some(m)) if m >= 0 => {
            let months = m.max(1);
            Some((remaining + months - 1) / months)
        }
        _ => None,
    };
    Goal {
        id,
        name,
        target_cents,
        target_date,
        account_id,
        account_name,
        current_cents,
        remaining_cents: remaining,
        progress: if target_cents > 0 { current_cents as f64 / target_cents as f64 } else { 0.0 },
        reached,
        months_left,
        monthly_needed_cents,
        overdue,
    }
}

pub fn list(conn: &Connection, scope: Scope) -> Result<Vec<Goal>> {
    let day = today();
    let mut stmt = conn.prepare(
        "SELECT g.id, g.name, g.target_cents, g.target_date, g.account_id, a.name, a.is_test,
                (SELECT b.amount_cents FROM balances b WHERE b.account_id = a.id
                 ORDER BY b.as_of DESC, b.id DESC LIMIT 1)
         FROM goals g LEFT JOIN accounts a ON a.id = g.account_id
         ORDER BY g.target_date IS NULL, g.target_date, g.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let is_test: Option<bool> = r.get(6)?;
        Ok((
            build(
                Row {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    target_cents: r.get(2)?,
                    target_date: r.get(3)?,
                    account_id: r.get(4)?,
                    account_name: r.get(5)?,
                    current_cents: r.get::<_, Option<i64>>(7)?.unwrap_or(0),
                },
                &day,
            ),
            is_test,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (goal, is_test) = row?;
        // A goal on a test account belongs to the Sandbox world; one with no
        // account at all shows everywhere.
        if is_test.is_none_or(|t| scope.matches(t)) {
            out.push(goal);
        }
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: i64) -> Result<Goal> {
    list(conn, Scope::All)?
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| CoreError::Invalid("Goal not found.".into()))
}

pub fn create(conn: &Connection, input: &GoalInput) -> Result<Goal> {
    let c = validate(conn, input)?;
    conn.execute(
        "INSERT INTO goals (name, target_cents, target_date, account_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![c.name, input.target_cents, c.target_date, input.account_id, now_rfc3339()],
    )?;
    get(conn, conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &GoalInput) -> Result<Goal> {
    let c = validate(conn, input)?;
    let n = conn.execute(
        "UPDATE goals SET name = ?1, target_cents = ?2, target_date = ?3, account_id = ?4 WHERE id = ?5",
        params![c.name, input.target_cents, c.target_date, input.account_id, id],
    )?;
    if n == 0 {
        return Err(CoreError::Invalid("Goal not found.".into()));
    }
    get(conn, id)
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM goals WHERE id = ?1", [id])?;
    Ok(())
}

/// Goals whose linked account no longer exists show 0 saved; this reports
/// them so the UI can nudge.
pub fn orphaned(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT count(*) FROM goals WHERE account_id IS NULL",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::tests::{conn, input};

    fn goal(name: &str, target: i64, date: Option<&str>, account: Option<i64>) -> GoalInput {
        GoalInput {
            name: name.into(),
            target_cents: target,
            target_date: date.map(String::from),
            account_id: account,
        }
    }

    #[test]
    fn months_math() {
        assert_eq!(months_between("2026-09-17", "2027-09-01").unwrap(), 12);
        assert_eq!(months_between("2026-09-17", "2026-09-30").unwrap(), 1);
        assert_eq!(months_between("2026-09-17", "2026-09-01").unwrap(), 0);
        assert_eq!(months_between("2026-09-17", "2026-06-01").unwrap(), -3);
    }

    #[test]
    fn progress_follows_the_linked_account() {
        let mut c = conn();
        let mut hysa = input("Ally HYSA", "hysa");
        hysa.balance_cents = Some(1_200_000);
        let hysa = accounts::create(&mut c, &hysa).unwrap();

        let g = create(&c, &goal("House down payment", 6_000_000, None, Some(hysa.id))).unwrap();
        assert_eq!(g.current_cents, 1_200_000);
        assert_eq!(g.remaining_cents, 4_800_000);
        assert!((g.progress - 0.2).abs() < 1e-9);
        assert_eq!(g.account_name.as_deref(), Some("Ally HYSA"));
        assert!(!g.reached && g.monthly_needed_cents.is_none(), "no date means no monthly figure");

        // Saving more moves the goal along.
        accounts::set_balance(&c, hysa.id, &today(), 3_000_000).unwrap();
        assert_eq!(get(&c, g.id).unwrap().current_cents, 3_000_000);

        // Reaching the target.
        accounts::set_balance(&c, hysa.id, &today(), 6_500_000).unwrap();
        let done = get(&c, g.id).unwrap();
        assert!(done.reached && done.remaining_cents == 0 && done.monthly_needed_cents.is_none());
    }

    #[test]
    fn monthly_amount_needed_and_overdue() {
        let mut c = conn();
        let mut hysa = input("Ally HYSA", "hysa");
        hysa.balance_cents = Some(100_000);
        let hysa = accounts::create(&mut c, &hysa).unwrap();
        let in_ten = crate::budgets::shift_month(&crate::budgets::month_of(&today()).unwrap(), 10).unwrap();
        let g = create(&c, &goal("Emergency fund", 1_100_000, Some(&format!("{in_ten}-01")), Some(hysa.id)))
            .unwrap();
        // $10,000 left over about ten months.
        let monthly = g.monthly_needed_cents.unwrap();
        assert!((95_000..=112_000).contains(&monthly), "got {monthly}");
        assert!(!g.overdue);

        let past = create(&c, &goal("Late goal", 500_000, Some("2020-01-01"), Some(hysa.id))).unwrap();
        assert!(past.overdue);
        assert_eq!(past.monthly_needed_cents, None);
    }

    #[test]
    fn validation_and_scope() {
        let mut c = conn();
        let hysa = accounts::create(&mut c, &input("HYSA", "hysa")).unwrap();
        let loan = accounts::create(&mut c, &input("Car loan", "loan")).unwrap();
        let test_acct = accounts::create(&mut c, &input("Plaid HYSA", "hysa")).unwrap();
        accounts::set_is_test(&c, test_acct.id, true).unwrap();

        assert!(create(&c, &goal("", 1000, None, None)).is_err());
        assert!(create(&c, &goal("Zero", 0, None, None)).is_err());
        assert!(create(&c, &goal("Bad date", 1000, Some("soon"), None)).is_err());
        assert!(create(&c, &goal("On a debt", 1000, None, Some(loan.id))).is_err());
        assert!(create(&c, &goal("Missing account", 1000, None, Some(9_999))).is_err());

        create(&c, &goal("Real", 1000, None, Some(hysa.id))).unwrap();
        create(&c, &goal("Test world", 1000, None, Some(test_acct.id))).unwrap();
        create(&c, &goal("No account", 1000, None, None)).unwrap();
        let real: Vec<String> = list(&c, Scope::Real).unwrap().into_iter().map(|g| g.name).collect();
        assert_eq!(real, vec!["No account", "Real"]);
        let test: Vec<String> = list(&c, Scope::Test).unwrap().into_iter().map(|g| g.name).collect();
        assert_eq!(test, vec!["No account", "Test world"]);

        let g = create(&c, &goal("Rename me", 1000, None, None)).unwrap();
        let g = update(&c, g.id, &goal("Renamed", 2000, None, Some(hysa.id))).unwrap();
        assert_eq!((g.name.as_str(), g.target_cents), ("Renamed", 2000));
        delete(&c, g.id).unwrap();
        assert!(get(&c, g.id).is_err());
    }
}
