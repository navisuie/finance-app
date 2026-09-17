//! Accounts and balance snapshots.
//!
//! Balances are stored as the institution shows them: a credit card with $500
//! owed has a balance of +500.00 and `is_asset = false`. Net worth is
//! assets minus liabilities.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::util::{now_rfc3339, optional_text, parse_date, required_text};

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
    pub balance_cents: Option<i64>,
    pub balance_as_of: Option<String>,
    pub transaction_count: i64,
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
    Ok(Clean { name, kind, institution, last4, is_asset })
}

const SELECT: &str = "
    SELECT a.id, a.name, a.kind, a.institution, a.last4, a.is_asset, a.archived,
           a.plaid_account_id IS NOT NULL,
           b.amount_cents, b.as_of,
           (SELECT count(*) FROM transactions t WHERE t.account_id = a.id)
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
        balance_cents: r.get(8)?,
        balance_as_of: r.get(9)?,
        transaction_count: r.get(10)?,
    })
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Account>> {
    let sql = format!(
        "{SELECT} WHERE (?1 OR a.archived = 0) ORDER BY a.archived, a.is_asset DESC, a.name COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([include_archived], map_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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
        "INSERT INTO accounts (name, kind, institution, last4, is_asset, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![c.name, c.kind, c.institution, c.last4, c.is_asset, now_rfc3339()],
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
        "UPDATE accounts SET name = ?1, kind = ?2, institution = ?3, last4 = ?4, is_asset = ?5
         WHERE id = ?6",
        params![c.name, c.kind, c.institution, c.last4, c.is_asset, id],
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

/// Current net worth from each open account's latest balance.
pub fn net_worth(conn: &Connection) -> Result<NetWorth> {
    let mut nw = NetWorth::default();
    for a in list(conn, false)? {
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
        let nw = net_worth(&c).unwrap();
        assert_eq!(nw, NetWorth { assets_cents: 300_000, liabilities_cents: 50_000, net_cents: 250_000 });

        set_archived(&c, card.id, true).unwrap();
        assert_eq!(net_worth(&c).unwrap().net_cents, 300_000);
        assert_eq!(list(&c, false).unwrap().len(), 1);
        assert_eq!(list(&c, true).unwrap().len(), 2);
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
