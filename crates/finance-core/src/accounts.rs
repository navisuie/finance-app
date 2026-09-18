//! Accounts and balance snapshots.
//!
//! Balances are stored as the institution shows them: a credit card with $500
//! owed has a balance of +500.00 and `is_asset = false`. Net worth is
//! assets minus liabilities.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::util::{now_rfc3339, optional_text, parse_date, required_text};

/// Which set of data a query covers. The app shows test data while Plaid
/// Sandbox is active and real data otherwise, so the two never blend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Real accounts only (the default).
    #[default]
    Real,
    /// Accounts created by a Plaid Sandbox connection.
    Test,
    /// Everything, used where both make sense (account pickers, exports).
    All,
}

impl Scope {
    /// SQL fragment for the `accounts` alias `a`.
    pub fn sql(self) -> &'static str {
        match self {
            Scope::Real => "a.is_test = 0",
            Scope::Test => "a.is_test = 1",
            Scope::All => "1 = 1",
        }
    }

    pub fn matches(self, is_test: bool) -> bool {
        match self {
            Scope::Real => !is_test,
            Scope::Test => is_test,
            Scope::All => true,
        }
    }
}

pub const KINDS: &[&str] = &[
    "checking", "savings", "hysa", "brokerage", "401k", "roth_ira", "traditional_ira", "hsa",
    "credit_card", "loan", "mortgage", "other",
];

pub fn default_is_asset(kind: &str) -> bool {
    !matches!(kind, "credit_card" | "loan" | "mortgage")
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub institution: Option<String>,
    pub last4: Option<String>,
    pub is_asset: bool,
    pub archived: bool,
    pub linked: bool,
    /// Created from a Plaid Sandbox connection: fake data, kept out of net
    /// worth and the dashboard.
    pub is_test: bool,
    pub balance_cents: Option<i64>,
    pub balance_as_of: Option<String>,
    pub transaction_count: i64,
    /// Annual rate in basis points (6.49% = 649), for debts.
    pub apr_bps: Option<i64>,
    /// What you usually pay each month, for debts.
    pub monthly_payment_cents: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    pub name: String,
    pub kind: String,
    pub institution: Option<String>,
    pub last4: Option<String>,
    /// Defaults from `kind` when omitted.
    pub is_asset: Option<bool>,
    /// Optional opening balance, recorded as a snapshot.
    pub balance_cents: Option<i64>,
    pub balance_as_of: Option<String>,
    pub apr_bps: Option<i64>,
    pub monthly_payment_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSnapshot {
    pub id: i64,
    pub account_id: i64,
    pub as_of: String,
    pub amount_cents: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetWorth {
    pub assets_cents: i64,
    pub liabilities_cents: i64,
    pub net_cents: i64,
}

struct Clean {
    name: String,
    kind: String,
    institution: Option<String>,
    last4: Option<String>,
    is_asset: bool,
    apr_bps: Option<i64>,
    monthly_payment_cents: Option<i64>,
}

fn validate(input: &AccountInput) -> Result<Clean> {
    let name = required_text("Account name", &input.name, 100)?;
    let kind = input.kind.trim().to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(CoreError::Invalid(format!("Unknown account type '{kind}'.")));
    }
    let institution = optional_text("Institution", input.institution.as_deref(), 100)?;
    let last4 = optional_text("Last 4 digits", input.last4.as_deref(), 4)?;
    if let Some(l) = &last4 {
        if !l.chars().all(|c| c.is_ascii_digit()) {
            return Err(CoreError::Invalid(
                "Enter only the last 4 digits of the account number.".into(),
            ));
        }
    }
    let is_asset = input.is_asset.unwrap_or_else(|| default_is_asset(&kind));
    // 0% to 500%, to catch a rate typed as 649 instead of 6.49.
    if let Some(apr) = input.apr_bps {
        if !(0..=50_000).contains(&apr) {
            return Err(CoreError::Invalid("Enter a rate between 0% and 500%.".into()));
        }
    }
    if let Some(p) = input.monthly_payment_cents {
        if !(0..=crate::transactions::MAX_AMOUNT_CENTS).contains(&p) {
            return Err(CoreError::Invalid("That payment amount doesn't look right.".into()));
        }
    }
    Ok(Clean {
        name,
        kind,
        institution,
        last4,
        is_asset,
        apr_bps: input.apr_bps,
        monthly_payment_cents: input.monthly_payment_cents,
    })
}

const SELECT: &str = "
    SELECT a.id, a.name, a.kind, a.institution, a.last4, a.is_asset, a.archived,
           a.plaid_account_id IS NOT NULL, a.is_test,
           b.amount_cents, b.as_of,
           (SELECT count(*) FROM transactions t WHERE t.account_id = a.id),
           a.apr_bps, a.monthly_payment_cents
    FROM accounts a
    LEFT JOIN balances b ON b.id = (
        SELECT id FROM balances WHERE account_id = a.id ORDER BY as_of DESC, id DESC LIMIT 1
    )";

fn map_row(r: &Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        institution: r.get(3)?,
        last4: r.get(4)?,
        is_asset: r.get(5)?,
        archived: r.get(6)?,
        linked: r.get(7)?,
        is_test: r.get(8)?,
        balance_cents: r.get(9)?,
        balance_as_of: r.get(10)?,
        transaction_count: r.get(11)?,
        apr_bps: r.get(12)?,
        monthly_payment_cents: r.get(13)?,
    })
}

/// List accounts within a scope.
pub fn list(conn: &Connection, include_archived: bool, scope: Scope) -> Result<Vec<Account>> {
    let sql = format!(
        "{SELECT} WHERE (?1 OR a.archived = 0) AND {}
         ORDER BY a.archived, a.is_test, a.is_asset DESC, a.name COLLATE NOCASE",
        scope.sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([include_archived], map_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Mark an account as test data (or not).
pub fn set_is_test(conn: &Connection, id: i64, is_test: bool) -> Result<()> {
    conn.execute("UPDATE accounts SET is_test = ?1 WHERE id = ?2", params![is_test, id])?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TestDataSummary {
    pub accounts: i64,
    pub transactions: i64,
}

pub fn test_data_summary(conn: &Connection) -> Result<TestDataSummary> {
    Ok(conn.query_row(
        "SELECT (SELECT count(*) FROM accounts WHERE is_test = 1),
                (SELECT count(*) FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE a.is_test = 1)",
        [],
        |r| Ok(TestDataSummary { accounts: r.get(0)?, transactions: r.get(1)? }),
    )?)
}

/// Delete every test account with its transactions, balances and holdings.
/// Real accounts are never touched.
pub fn delete_test_data(conn: &mut Connection) -> Result<TestDataSummary> {
    let summary = test_data_summary(conn)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM accounts WHERE is_test = 1", [])?;
    tx.execute("DELETE FROM plaid_items WHERE access_token LIKE 'access-sandbox-%'", [])?;
    tx.commit()?;
    Ok(summary)
}

pub fn get(conn: &Connection, id: i64) -> Result<Account> {
    conn.query_row(&format!("{SELECT} WHERE a.id = ?1"), [id], map_row)
        .optional()?
        .ok_or_else(|| CoreError::Invalid("Account not found.".into()))
}

pub fn create(conn: &mut Connection, input: &AccountInput) -> Result<Account> {
    let tx = conn.transaction()?;
    let id = insert(&tx, input)?;
    tx.commit()?;
    get(conn, id)
}

/// Insert an account (and optional opening balance) without starting a
/// transaction; callers that need atomicity wrap it in one.
pub fn insert(conn: &Connection, input: &AccountInput) -> Result<i64> {
    let c = validate(input)?;
    conn.execute(
        "INSERT INTO accounts (name, kind, institution, last4, is_asset, apr_bps,
                               monthly_payment_cents, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            c.name, c.kind, c.institution, c.last4, c.is_asset, c.apr_bps, c.monthly_payment_cents,
            now_rfc3339()
        ],
    )?;
    let id = conn.last_insert_rowid();
    if let Some(cents) = input.balance_cents {
        let as_of = match &input.balance_as_of {
            Some(d) => parse_date(d)?,
            None => crate::util::today(),
        };
        upsert_balance(conn, id, &as_of, cents, "manual")?;
    }
    Ok(id)
}

pub fn update(conn: &Connection, id: i64, input: &AccountInput) -> Result<Account> {
    let c = validate(input)?;
    let n = conn.execute(
        "UPDATE accounts SET name = ?1, kind = ?2, institution = ?3, last4 = ?4, is_asset = ?5,
                apr_bps = ?6, monthly_payment_cents = ?7
         WHERE id = ?8",
        params![c.name, c.kind, c.institution, c.last4, c.is_asset, c.apr_bps, c.monthly_payment_cents, id],
    )?;
    if n == 0 {
        return Err(CoreError::Invalid("Account not found.".into()));
    }
    get(conn, id)
}

pub fn set_archived(conn: &Connection, id: i64, archived: bool) -> Result<Account> {
    conn.execute("UPDATE accounts SET archived = ?1 WHERE id = ?2", params![archived, id])?;
    get(conn, id)
}

/// Permanently delete an account with its balances and transactions.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(CoreError::Invalid("Account not found.".into()));
    }
    Ok(())
}

fn upsert_balance(conn: &Connection, account_id: i64, as_of: &str, cents: i64, source: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO balances (account_id, as_of, amount_cents, source) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(account_id, as_of) DO UPDATE SET amount_cents = excluded.amount_cents,
                                                     source = excluded.source",
        params![account_id, as_of, cents, source],
    )?;
    Ok(())
}

/// Record the balance on a date (replaces any snapshot for that date).
pub fn set_balance(conn: &Connection, account_id: i64, as_of: &str, cents: i64) -> Result<Account> {
    get(conn, account_id)?;
    let as_of = parse_date(as_of)?;
    upsert_balance(conn, account_id, &as_of, cents, "manual")?;
    get(conn, account_id)
}

pub fn delete_balance(conn: &Connection, balance_id: i64) -> Result<()> {
    conn.execute("DELETE FROM balances WHERE id = ?1", [balance_id])?;
    Ok(())
}

pub fn balance_history(conn: &Connection, account_id: i64) -> Result<Vec<BalanceSnapshot>> {
    let mut stmt = conn.prepare(
        "SELECT id, account_id, as_of, amount_cents, source FROM balances
         WHERE account_id = ?1 ORDER BY as_of DESC, id DESC",
    )?;
    let rows = stmt.query_map([account_id], |r| {
        Ok(BalanceSnapshot {
            id: r.get(0)?,
            account_id: r.get(1)?,
            as_of: r.get(2)?,
            amount_cents: r.get(3)?,
            source: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Current net worth from each open account's latest balance, within a scope.
pub fn net_worth(conn: &Connection, scope: Scope) -> Result<NetWorth> {
    let mut nw = NetWorth::default();
    for a in list(conn, false, scope)? {
        let bal = a.balance_cents.unwrap_or(0);
        if a.is_asset {
            nw.assets_cents += bal;
        } else {
            nw.liabilities_cents += bal;
        }
    }
    nw.net_cents = nw.assets_cents - nw.liabilities_cents;
    Ok(nw)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetWorthPoint {
    /// `YYYY-MM`.
    pub month: String,
    pub assets_cents: i64,
    pub liabilities_cents: i64,
    pub net_cents: i64,
    /// False until the first balance was recorded, so the chart can start there.
    pub has_data: bool,
}

/// Net worth at the end of each of the last `months` months, from the latest
/// balance recorded for each account on or before that date. Archived
/// accounts count for the months they were open.
pub fn net_worth_history(conn: &Connection, scope: Scope, months: u32) -> Result<Vec<NetWorthPoint>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT b.account_id, a.is_asset, b.as_of, b.amount_cents
         FROM balances b JOIN accounts a ON a.id = b.account_id
         WHERE {} ORDER BY b.as_of, b.id",
        scope.sql()
    ))?;
    let rows: Vec<(i64, bool, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;

    let today = crate::util::today();
    let this_month = crate::budgets::month_of(&today)?;
    let mut out = Vec::new();
    // Latest balance per account as we walk forward through time.
    let mut latest: std::collections::HashMap<i64, (bool, i64)> = std::collections::HashMap::new();
    let mut next = 0usize;
    let start = -(months as i32) + 1;
    for offset in start..=0 {
        let month = crate::budgets::shift_month(&this_month, offset)?;
        let (_, month_end) = crate::budgets::month_range(&month)?;
        while next < rows.len() && rows[next].2 <= month_end {
            let (account_id, is_asset, _, amount) = &rows[next];
            latest.insert(*account_id, (*is_asset, *amount));
            next += 1;
        }
        let mut assets = 0;
        let mut liabilities = 0;
        for (is_asset, amount) in latest.values() {
            if *is_asset {
                assets += amount;
            } else {
                liabilities += amount;
            }
        }
        out.push(NetWorthPoint {
            month,
            assets_cents: assets,
            liabilities_cents: liabilities,
            net_cents: assets - liabilities,
            has_data: !latest.is_empty(),
        });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::migrate;

    pub fn conn() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&mut c).unwrap();
        c
    }

    pub fn input(name: &str, kind: &str) -> AccountInput {
        AccountInput {
            name: name.into(),
            kind: kind.into(),
            institution: None,
            last4: None,
            is_asset: None,
            balance_cents: None,
            balance_as_of: None,
            apr_bps: None,
            monthly_payment_cents: None,
        }
    }

    #[test]
    fn create_with_opening_balance() {
        let mut c = conn();
        let mut i = input("  Ally HYSA ", "hysa");
        i.last4 = Some("1234".into());
        i.balance_cents = Some(1_500_000);
        i.balance_as_of = Some("2026-09-01".into());
        let a = create(&mut c, &i).unwrap();
        assert_eq!(a.name, "Ally HYSA");
        assert!(a.is_asset);
        assert_eq!(a.balance_cents, Some(1_500_000));
        assert_eq!(a.balance_as_of.as_deref(), Some("2026-09-01"));
    }

    #[test]
    fn debt_terms_round_trip() {
        let mut c = conn();
        let mut i = input("Car loan", "loan");
        i.apr_bps = Some(649);
        i.monthly_payment_cents = Some(45_000);
        i.balance_cents = Some(1_800_000);
        let a = create(&mut c, &i).unwrap();
        assert_eq!((a.apr_bps, a.monthly_payment_cents), (Some(649), Some(45_000)));
        assert!(!a.is_asset);

        let mut bad = input("Car loan 2", "loan");
        bad.apr_bps = Some(60_000);
        assert!(create(&mut c, &bad).is_err());
        bad.apr_bps = Some(-1);
        assert!(create(&mut c, &bad).is_err());

        let mut edit = input("Car loan", "loan");
        edit.apr_bps = Some(500);
        edit.monthly_payment_cents = Some(50_000);
        let a = update(&c, a.id, &edit).unwrap();
        assert_eq!((a.apr_bps, a.monthly_payment_cents), (Some(500), Some(50_000)));
    }

    #[test]
    fn validation() {
        let mut c = conn();
        assert!(create(&mut c, &input("", "hysa")).is_err());
        assert!(create(&mut c, &input("X", "casino")).is_err());
        let mut i = input("X", "checking");
        i.last4 = Some("12345".into());
        assert!(create(&mut c, &i).is_err());
        i.last4 = Some("12a4".into());
        assert!(create(&mut c, &i).is_err());
    }

    #[test]
    fn liabilities_default_and_net_worth() {
        let mut c = conn();
        let mut chk = input("Checking", "checking");
        chk.balance_cents = Some(300_000);
        create(&mut c, &chk).unwrap();
        let mut card = input("Card", "credit_card");
        card.balance_cents = Some(50_000);
        let card = create(&mut c, &card).unwrap();
        assert!(!card.is_asset);
        let nw = net_worth(&c, Scope::Real).unwrap();
        assert_eq!(nw, NetWorth { assets_cents: 300_000, liabilities_cents: 50_000, net_cents: 250_000 });

        set_archived(&c, card.id, true).unwrap();
        assert_eq!(net_worth(&c, Scope::Real).unwrap().net_cents, 300_000);
        assert_eq!(list(&c, false, Scope::Real).unwrap().len(), 1);
        assert_eq!(list(&c, true, Scope::Real).unwrap().len(), 2);
    }

    #[test]
    fn latest_balance_wins_and_same_day_replaces() {
        let mut c = conn();
        let a = create(&mut c, &input("Brokerage", "brokerage")).unwrap();
        set_balance(&c, a.id, "2026-08-01", 100).unwrap();
        set_balance(&c, a.id, "2026-09-01", 200).unwrap();
        set_balance(&c, a.id, "2026-07-01", 50).unwrap();
        assert_eq!(get(&c, a.id).unwrap().balance_cents, Some(200));
        set_balance(&c, a.id, "2026-09-01", 250).unwrap();
        let h = balance_history(&c, a.id).unwrap();
        assert_eq!(h.len(), 3);
        assert_eq!(h[0].amount_cents, 250);
        delete_balance(&c, h[0].id).unwrap();
        assert_eq!(get(&c, a.id).unwrap().balance_cents, Some(100));
    }

    #[test]
    fn test_accounts_are_hidden_and_removable() {
        let mut c = conn();
        let mut real = input("Real checking", "checking");
        real.balance_cents = Some(100_000);
        let real = create(&mut c, &real).unwrap();
        let mut fake = input("Plaid Checking", "checking");
        fake.balance_cents = Some(999_999);
        let fake = create(&mut c, &fake).unwrap();
        set_is_test(&c, fake.id, true).unwrap();

        assert_eq!(list(&c, true, Scope::Real).unwrap().len(), 1);
        assert_eq!(list(&c, true, Scope::Test).unwrap().len(), 1);
        assert_eq!(list(&c, true, Scope::All).unwrap().len(), 2);
        assert!(list(&c, true, Scope::Test).unwrap().iter().all(|a| a.is_test));
        assert_eq!(net_worth(&c, Scope::Real).unwrap().net_cents, 100_000, "real net worth ignores test data");
        assert_eq!(net_worth(&c, Scope::Test).unwrap().net_cents, 999_999, "Sandbox has its own totals");

        assert_eq!(test_data_summary(&c).unwrap().accounts, 1);
        let removed = delete_test_data(&mut c).unwrap();
        assert_eq!(removed.accounts, 1);
        assert_eq!(list(&c, true, Scope::All).unwrap().len(), 1);
        assert_eq!(get(&c, real.id).unwrap().name, "Real checking");
    }

    #[test]
    fn net_worth_history_walks_forward_from_snapshots() {
        let mut c = conn();
        let chk = create(&mut c, &input("Checking", "checking")).unwrap();
        let card = create(&mut c, &input("Card", "credit_card")).unwrap();
        let today = crate::util::today();
        let this_month = crate::budgets::month_of(&today).unwrap();
        let m = |back: i32| crate::budgets::shift_month(&this_month, back).unwrap();

        set_balance(&c, chk.id, &format!("{}-05", m(-3)), 100_000).unwrap();
        set_balance(&c, card.id, &format!("{}-06", m(-2)), 20_000).unwrap();
        set_balance(&c, chk.id, &format!("{}-07", m(-1)), 150_000).unwrap();

        let h = net_worth_history(&c, Scope::Real, 5).unwrap();
        assert_eq!(h.len(), 5);
        assert_eq!(h[0].month, m(-4));
        assert!(!h[0].has_data, "nothing recorded yet");
        assert_eq!(h[1].net_cents, 100_000);
        assert_eq!(h[2].net_cents, 80_000, "the card counts against it");
        assert_eq!(h[3].net_cents, 130_000, "a newer balance replaces the old one");
        assert_eq!(h[4].net_cents, 130_000, "months with no change carry forward");
        assert_eq!(h[4].assets_cents, 150_000);
        assert_eq!(h[4].liabilities_cents, 20_000);

        // An archived account still counts for the months it held money.
        set_archived(&c, card.id, true).unwrap();
        assert_eq!(net_worth_history(&c, Scope::Real, 5).unwrap()[4].liabilities_cents, 20_000);
    }

    #[test]
    fn delete_cascades() {
        let mut c = conn();
        let a = create(&mut c, &input("Temp", "checking")).unwrap();
        set_balance(&c, a.id, "2026-09-01", 1).unwrap();
        delete(&c, a.id).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM balances", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert!(get(&c, a.id).is_err());
    }
}
