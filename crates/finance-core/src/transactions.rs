//! Transactions: manual entry, editing, querying.
//!
//! Sign convention: negative amounts are money leaving the account (spending,
//! or a charge on a credit card); positive amounts are money coming in
//! (income, refunds, card payments).

use std::collections::HashMap;

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::accounts::Scope;
use crate::error::{CoreError, Result};
use crate::util::{normalize_payee, now_rfc3339, optional_text, parse_date, required_text};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: i64,
    pub account_id: i64,
    pub account_name: String,
    pub posted_on: String,
    pub amount_cents: i64,
    pub payee: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub notes: Option<String>,
    pub pending: bool,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInput {
    pub account_id: i64,
    pub posted_on: String,
    pub amount_cents: i64,
    pub payee: String,
    pub category_id: Option<i64>,
    pub notes: Option<String>,
}

/// `category_id = Some(0)` selects uncategorized transactions.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TransactionFilter {
    pub account_id: Option<i64>,
    pub category_id: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub search: Option<String>,
    /// Which data set to cover; the app fills this in from the active Plaid
    /// environment when the UI doesn't ask for something specific.
    pub scope: Option<Scope>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransactionPage {
    pub items: Vec<Transaction>,
    pub total: i64,
    pub inflow_cents: i64,
    pub outflow_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PeriodSummary {
    pub income_cents: i64,
    /// Positive number: total spent (excludes transfers).
    pub spending_cents: i64,
    pub transaction_count: i64,
}

pub(crate) const MAX_AMOUNT_CENTS: i64 = 1_000_000_000_000; // $10 billion

pub(crate) fn check_amount(cents: i64) -> Result<()> {
    if cents.abs() > MAX_AMOUNT_CENTS {
        return Err(CoreError::Invalid("That amount is too large.".into()));
    }
    Ok(())
}

struct Clean {
    posted_on: String,
    payee: String,
    notes: Option<String>,
}

fn validate(conn: &Connection, input: &TransactionInput) -> Result<Clean> {
    check_amount(input.amount_cents)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ?1)",
        [input.account_id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(CoreError::Invalid("Account not found.".into()));
    }
    if let Some(cid) = input.category_id {
        let ok: bool =
            conn.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)", [cid], |r| r.get(0))?;
        if !ok {
            return Err(CoreError::Invalid("Category not found.".into()));
        }
    }
    Ok(Clean {
        posted_on: parse_date(&input.posted_on)?,
        payee: required_text("Payee", &input.payee, 200)?,
        notes: optional_text("Notes", input.notes.as_deref(), 1000)?,
    })
}

const SELECT: &str = "
    SELECT t.id, t.account_id, a.name, t.posted_on, t.amount_cents, t.payee,
           t.category_id, c.name, t.notes, t.pending, t.source
    FROM transactions t
    JOIN accounts a ON a.id = t.account_id
    LEFT JOIN categories c ON c.id = t.category_id";

fn map_row(r: &Row) -> rusqlite::Result<Transaction> {
    Ok(Transaction {
        id: r.get(0)?,
        account_id: r.get(1)?,
        account_name: r.get(2)?,
        posted_on: r.get(3)?,
        amount_cents: r.get(4)?,
        payee: r.get(5)?,
        category_id: r.get(6)?,
        category_name: r.get(7)?,
        notes: r.get(8)?,
        pending: r.get(9)?,
        source: r.get(10)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Transaction> {
    conn.query_row(&format!("{SELECT} WHERE t.id = ?1"), [id], map_row)
        .optional()?
        .ok_or_else(|| CoreError::Invalid("Transaction not found.".into()))
}

pub fn create(conn: &Connection, input: &TransactionInput) -> Result<Transaction> {
    let c = validate(conn, input)?;
    conn.execute(
        "INSERT INTO transactions (account_id, posted_on, amount_cents, payee, category_id, notes,
                                   source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual', ?7)",
        params![
            input.account_id, c.posted_on, input.amount_cents, c.payee, input.category_id, c.notes,
            now_rfc3339()
        ],
    )?;
    get(conn, conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &TransactionInput) -> Result<Transaction> {
    let c = validate(conn, input)?;
    let n = conn.execute(
        "UPDATE transactions SET account_id = ?1, posted_on = ?2, amount_cents = ?3, payee = ?4,
                category_id = ?5, notes = ?6 WHERE id = ?7",
        params![input.account_id, c.posted_on, input.amount_cents, c.payee, input.category_id, c.notes, id],
    )?;
    if n == 0 {
        return Err(CoreError::Invalid("Transaction not found.".into()));
    }
    get(conn, id)
}

pub fn delete_many(conn: &mut Connection, ids: &[i64]) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    {
        let mut stmt = tx.prepare("DELETE FROM transactions WHERE id = ?1")?;
        for id in ids {
            n += stmt.execute([id])?;
        }
    }
    tx.commit()?;
    Ok(n)
}

pub fn set_category(conn: &mut Connection, ids: &[i64], category_id: Option<i64>) -> Result<usize> {
    if let Some(cid) = category_id {
        let ok: bool =
            conn.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)", [cid], |r| r.get(0))?;
        if !ok {
            return Err(CoreError::Invalid("Category not found.".into()));
        }
    }
    let tx = conn.transaction()?;
    let mut n = 0;
    {
        let mut stmt = tx.prepare("UPDATE transactions SET category_id = ?1 WHERE id = ?2")?;
        for id in ids {
            n += stmt.execute(params![category_id, id])?;
        }
    }
    tx.commit()?;
    Ok(n)
}

fn where_clause(f: &TransactionFilter) -> Result<(String, Vec<Value>)> {
    let mut clauses = vec![f.scope.unwrap_or_default().sql().to_string()];
    let mut args: Vec<Value> = Vec::new();
    if let Some(a) = f.account_id {
        args.push(a.into());
        clauses.push(format!("t.account_id = ?{}", args.len()));
    }
    match f.category_id {
        Some(0) => clauses.push("t.category_id IS NULL".into()),
        Some(c) => {
            args.push(c.into());
            clauses.push(format!("t.category_id = ?{}", args.len()));
        }
        None => {}
    }
    if let Some(d) = f.from.as_deref().filter(|s| !s.is_empty()) {
        args.push(parse_date(d)?.into());
        clauses.push(format!("t.posted_on >= ?{}", args.len()));
    }
    if let Some(d) = f.to.as_deref().filter(|s| !s.is_empty()) {
        args.push(parse_date(d)?.into());
        clauses.push(format!("t.posted_on <= ?{}", args.len()));
    }
    if let Some(s) = f.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // Escape LIKE wildcards so user input is matched literally.
        let escaped = s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        args.push(format!("%{escaped}%").into());
        let n = args.len();
        clauses.push(format!(
            "(t.payee LIKE ?{n} ESCAPE '\\' OR t.notes LIKE ?{n} ESCAPE '\\')"
        ));
    }
    Ok((clauses.join(" AND "), args))
}

pub fn query(conn: &Connection, f: &TransactionFilter) -> Result<TransactionPage> {
    let (clause, args) = where_clause(f)?;
    let limit = f.limit.unwrap_or(100).clamp(1, 1000);
    let offset = f.offset.unwrap_or(0).max(0);

    let (total, inflow, outflow): (i64, i64, i64) = conn.query_row(
        &format!(
            "SELECT count(*),
                    coalesce(sum(CASE WHEN t.amount_cents > 0 THEN t.amount_cents END), 0),
                    coalesce(sum(CASE WHEN t.amount_cents < 0 THEN t.amount_cents END), 0)
             FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE {clause}"
        ),
        params_from_iter(args.iter()),
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE {clause} ORDER BY t.posted_on DESC, t.id DESC LIMIT {limit} OFFSET {offset}"
    ))?;
    let items = stmt
        .query_map(params_from_iter(args.iter()), map_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(TransactionPage { items, total, inflow_cents: inflow, outflow_cents: -outflow })
}

/// Income and spending between two dates (inclusive), excluding transfers
/// and liabilities' payments categorized as transfers.
pub fn summary(conn: &Connection, from: &str, to: &str, scope: Scope) -> Result<PeriodSummary> {
    let from = parse_date(from)?;
    let to = parse_date(to)?;
    Ok(conn.query_row(
        &format!(
        "SELECT
            coalesce(sum(CASE WHEN t.amount_cents > 0 AND coalesce(c.kind,'') = 'income'
                              THEN t.amount_cents END), 0),
            coalesce(sum(CASE WHEN t.amount_cents < 0 AND coalesce(c.kind,'expense') = 'expense'
                              THEN -t.amount_cents
                              WHEN t.amount_cents > 0 AND c.kind = 'expense'
                              THEN -t.amount_cents END), 0),
            count(*)
         FROM transactions t
         JOIN accounts a ON a.id = t.account_id
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE t.posted_on BETWEEN ?1 AND ?2 AND {}",
            scope.sql()
        ),
        params![from, to],
        |r| Ok(PeriodSummary { income_cents: r.get(0)?, spending_cents: r.get(1)?, transaction_count: r.get(2)? }),
    )?)
}

/// Normalised payee -> the category most recently used for it, built from
/// the latest 10,000 categorized transactions (test accounts included, so a
/// Sandbox run still learns nothing harmful).
pub fn payee_category_map(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT payee, category_id FROM transactions
         WHERE category_id IS NOT NULL ORDER BY posted_on DESC, id DESC LIMIT 10000",
    )?;
    let mut map = HashMap::new();
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let p: String = r.get(0)?;
        let key = normalize_payee(&p);
        if !key.is_empty() {
            map.entry(key).or_insert(r.get(1)?);
        }
    }
    Ok(map)
}

/// The category most recently used for a payee, for auto-categorizing.
pub fn suggest_category(conn: &Connection, payee: &str) -> Result<Option<i64>> {
    let key = normalize_payee(payee);
    if key.is_empty() {
        return Ok(None);
    }
    Ok(payee_category_map(conn)?.get(&key).copied())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::accounts::{self, tests::{conn, input}};
    use crate::categories;

    pub fn tx(account_id: i64, date: &str, cents: i64, payee: &str) -> TransactionInput {
        TransactionInput {
            account_id,
            posted_on: date.into(),
            amount_cents: cents,
            payee: payee.into(),
            category_id: None,
            notes: None,
        }
    }

    fn cat(c: &Connection, name: &str) -> i64 {
        categories::find_by_name(c, name).unwrap().unwrap()
    }

    #[test]
    fn create_update_delete() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let t = create(&c, &tx(a.id, "2026-09-02", -1250, " Coffee ")).unwrap();
        assert_eq!(t.payee, "Coffee");
        assert_eq!(t.account_name, "Checking");
        let mut u = tx(a.id, "2026-09-03", -1500, "Coffee shop");
        u.category_id = Some(cat(&c, "Dining"));
        u.notes = Some("with Sam".into());
        let t2 = update(&c, t.id, &u).unwrap();
        assert_eq!(t2.category_name.as_deref(), Some("Dining"));
        assert_eq!(delete_many(&mut c, &[t.id]).unwrap(), 1);
        assert!(get(&c, t.id).is_err());
    }

    #[test]
    fn validation() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        assert!(create(&c, &tx(999, "2026-09-02", 1, "x")).is_err());
        assert!(create(&c, &tx(a.id, "bad", 1, "x")).is_err());
        assert!(create(&c, &tx(a.id, "2026-09-02", 1, "  ")).is_err());
        assert!(create(&c, &tx(a.id, "2026-09-02", MAX_AMOUNT_CENTS + 1, "x")).is_err());
        let mut bad_cat = tx(a.id, "2026-09-02", 1, "x");
        bad_cat.category_id = Some(99_999);
        assert!(create(&c, &bad_cat).is_err());
    }

    #[test]
    fn query_filters_and_totals() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let b = accounts::create(&mut c, &input("Card", "credit_card")).unwrap();
        let groceries = cat(&c, "Groceries");
        let mut g = tx(a.id, "2026-09-01", -5000, "Whole Foods");
        g.category_id = Some(groceries);
        create(&c, &g).unwrap();
        create(&c, &tx(a.id, "2026-09-05", 300000, "Payroll")).unwrap();
        create(&c, &tx(b.id, "2026-08-20", -2000, "100% Juice_Bar")).unwrap();

        let all = query(&c, &TransactionFilter::default()).unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(all.items[0].payee, "Payroll"); // newest first
        assert_eq!(all.inflow_cents, 300000);
        assert_eq!(all.outflow_cents, 7000);

        let f = TransactionFilter { account_id: Some(a.id), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 2);
        let f = TransactionFilter { category_id: Some(0), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 2);
        let f = TransactionFilter { category_id: Some(groceries), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 1);
        let f = TransactionFilter { from: Some("2026-09-01".into()), to: Some("2026-09-02".into()), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 1);
        // LIKE wildcards are literal.
        let f = TransactionFilter { search: Some("100%".into()), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 1);
        let f = TransactionFilter { search: Some("%".into()), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 1);
        let f = TransactionFilter { search: Some("_".into()), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 1);
        // SQL injection attempt is just a search string.
        let f = TransactionFilter { search: Some("'; DROP TABLE transactions; --".into()), ..Default::default() };
        assert_eq!(query(&c, &f).unwrap().total, 0);
        assert_eq!(query(&c, &TransactionFilter::default()).unwrap().total, 3);
        // Paging
        let f = TransactionFilter { limit: Some(1), offset: Some(1), ..Default::default() };
        let p = query(&c, &f).unwrap();
        assert_eq!((p.items.len(), p.total), (1, 3));
        assert_eq!(p.items[0].payee, "Whole Foods");
    }

    #[test]
    fn bulk_category_and_suggestions() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let t1 = create(&c, &tx(a.id, "2026-09-01", -500, "STARBUCKS #1234")).unwrap();
        let t2 = create(&c, &tx(a.id, "2026-09-02", -600, "Target")).unwrap();
        let dining = cat(&c, "Dining");
        assert_eq!(set_category(&mut c, &[t1.id, t2.id], Some(dining)).unwrap(), 2);
        assert!(set_category(&mut c, &[t1.id], Some(12345)).is_err());
        assert_eq!(suggest_category(&c, "Starbucks #9876").unwrap(), Some(dining));
        assert_eq!(suggest_category(&c, "Unknown place").unwrap(), None);
        set_category(&mut c, &[t2.id], None).unwrap();
        assert_eq!(get(&c, t2.id).unwrap().category_id, None);
    }

    #[test]
    fn period_summary() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let mut pay = tx(a.id, "2026-09-01", 400000, "Payroll");
        pay.category_id = Some(cat(&c, "Income"));
        create(&c, &pay).unwrap();
        create(&c, &tx(a.id, "2026-09-02", -10000, "Uncategorized spend")).unwrap();
        let mut refund = tx(a.id, "2026-09-03", 2000, "Refund");
        refund.category_id = Some(cat(&c, "Shopping"));
        create(&c, &refund).unwrap();
        let mut xfer = tx(a.id, "2026-09-04", -50000, "To HYSA");
        xfer.category_id = Some(cat(&c, "Transfer"));
        create(&c, &xfer).unwrap();
        create(&c, &tx(a.id, "2026-10-01", -999, "Next month")).unwrap();

        let s = summary(&c, "2026-09-01", "2026-09-30", Scope::Real).unwrap();
        assert_eq!(s.income_cents, 400000);
        assert_eq!(s.spending_cents, 10000 - 2000); // refund offsets spending; transfer excluded
        assert_eq!(s.transaction_count, 4);
    }
}
