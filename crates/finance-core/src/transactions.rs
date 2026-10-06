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
    /// Set when this moved money to another account you track (a loan or card
    /// payment, a transfer to savings) instead of being spent.
    pub transfer_account_id: Option<i64>,
    pub transfer_account_name: Option<String>,
    /// Set once this transaction is confirmed as one leg of a transfer;
    /// points at the other leg. See `crate::transfers`.
    pub linked_transaction_id: Option<i64>,
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
    /// The account this money went to, if it's a transfer.
    pub transfer_account_id: Option<i64>,
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
    /// Money moved into savings, brokerage or retirement accounts. Already
    /// out of `spending_cents` — reported so saving and investing can be
    /// shown for what they are rather than quietly left out.
    pub saved_cents: i64,
    /// How much of `spending_cents` has no category. Uncategorized money out
    /// counts as spending, so this is the usual reason a savings rate looks
    /// wrong.
    pub uncategorized_spending_cents: i64,
}

/// Account kinds that count as saving or investing when money is moved into them.
pub const SAVING_KINDS: &[&str] =
    &["savings", "hysa", "brokerage", "401k", "roth_ira", "traditional_ira", "hsa"];

/// Money tagged as moved into a savings or investment account between two
/// dates (inclusive).
pub fn saved_between(conn: &Connection, from: &str, to: &str, scope: Scope) -> Result<i64> {
    let kinds = SAVING_KINDS.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
    Ok(conn.query_row(
        &format!(
            "SELECT coalesce(sum(-t.amount_cents), 0)
             FROM transactions t
             JOIN accounts a ON a.id = t.account_id
             JOIN accounts target ON target.id = t.transfer_account_id
             WHERE t.posted_on BETWEEN ?1 AND ?2 AND t.amount_cents < 0
               AND target.kind IN ({kinds}) AND {}",
            scope.sql()
        ),
        params![parse_date(from)?, parse_date(to)?],
        |r| r.get(0),
    )?)
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
    if let Some(target) = input.transfer_account_id {
        if target == input.account_id {
            return Err(CoreError::Invalid("A transfer has to point at a different account.".into()));
        }
        let ok: bool =
            conn.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ?1)", [target], |r| r.get(0))?;
        if !ok {
            return Err(CoreError::Invalid("That account no longer exists.".into()));
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
           t.category_id, c.name, t.notes, t.pending, t.source,
           t.transfer_account_id, ta.name, t.linked_transaction_id
    FROM transactions t
    JOIN accounts a ON a.id = t.account_id
    LEFT JOIN categories c ON c.id = t.category_id
    LEFT JOIN accounts ta ON ta.id = t.transfer_account_id";

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
        transfer_account_id: r.get(11)?,
        transfer_account_name: r.get(12)?,
        linked_transaction_id: r.get(13)?,
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
                                   transfer_account_id, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'manual', ?8)",
        params![
            input.account_id, c.posted_on, input.amount_cents, c.payee, input.category_id, c.notes,
            input.transfer_account_id, now_rfc3339()
        ],
    )?;
    get(conn, conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &TransactionInput) -> Result<Transaction> {
    let c = validate(conn, input)?;
    let n = conn.execute(
        "UPDATE transactions SET account_id = ?1, posted_on = ?2, amount_cents = ?3, payee = ?4,
                category_id = ?5, notes = ?6, transfer_account_id = ?7 WHERE id = ?8",
        params![
            input.account_id, c.posted_on, input.amount_cents, c.payee, input.category_id, c.notes,
            input.transfer_account_id, id
        ],
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
    let summary = conn.query_row(
        &format!(
        "SELECT
            coalesce(sum(CASE WHEN t.amount_cents > 0 AND coalesce(c.kind,'') = 'income'
                              THEN t.amount_cents END), 0),
            coalesce(sum(CASE WHEN t.amount_cents < 0 AND coalesce(c.kind,'expense') = 'expense'
                              THEN -t.amount_cents
                              WHEN t.amount_cents > 0 AND c.kind = 'expense'
                              THEN -t.amount_cents END), 0),
            count(*),
            coalesce(sum(CASE WHEN t.amount_cents < 0 AND t.category_id IS NULL
                              THEN -t.amount_cents END), 0)
         FROM transactions t
         JOIN accounts a ON a.id = t.account_id
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE t.posted_on BETWEEN ?1 AND ?2 AND t.transfer_account_id IS NULL AND {}",
            scope.sql()
        ),
        params![&from, &to],
        |r| {
            Ok(PeriodSummary {
                income_cents: r.get(0)?,
                spending_cents: r.get(1)?,
                transaction_count: r.get(2)?,
                uncategorized_spending_cents: r.get(3)?,
                saved_cents: 0, // filled in below
            })
        },
    )?;
    Ok(PeriodSummary { saved_cents: saved_between(conn, &from, &to, scope)?, ..summary })
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

/// (source account, normalised payee) -> the account that payee's money was
/// most recently sent to. This is what makes a recurring loan or card payment
/// recognisable the second time: once you've tagged "Honda Financial" in
/// Checking as paying the car loan, the next one can be tagged for you.
///
/// Keyed by source account as well as payee, because the same payee name in
/// a different account isn't necessarily the same arrangement.
pub fn payee_transfer_map(conn: &Connection) -> Result<HashMap<(i64, String), i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT account_id, payee, transfer_account_id FROM transactions
         WHERE transfer_account_id IS NOT NULL ORDER BY posted_on DESC, id DESC LIMIT 10000",
    )?;
    let mut map = HashMap::new();
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let account_id: i64 = r.get(0)?;
        let payee: String = r.get(1)?;
        let key = normalize_payee(&payee);
        if !key.is_empty() {
            map.entry((account_id, key)).or_insert(r.get(2)?);
        }
    }
    Ok(map)
}

/// The account this payee's money was last sent to from this account, for
/// recognising a recurring debt or savings payment.
pub fn suggest_transfer_account(conn: &Connection, account_id: i64, payee: &str) -> Result<Option<i64>> {
    let key = normalize_payee(payee);
    if key.is_empty() {
        return Ok(None);
    }
    Ok(payee_transfer_map(conn)?.get(&(account_id, key)).copied())
}

/// Payments you've tagged as going to `account_id` (a debt, or any account
/// you track), newest first.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LinkedPayment {
    pub id: i64,
    pub posted_on: String,
    /// Positive: what the payment was worth.
    pub amount_cents: i64,
    pub payee: String,
    pub from_account_id: i64,
    pub from_account_name: String,
}

pub fn payments_to(conn: &Connection, account_id: i64, limit: i64) -> Result<Vec<LinkedPayment>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.posted_on, -t.amount_cents, t.payee, t.account_id, a.name
         FROM transactions t JOIN accounts a ON a.id = t.account_id
         WHERE t.transfer_account_id = ?1 AND t.amount_cents < 0
         ORDER BY t.posted_on DESC, t.id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![account_id, limit.clamp(1, 100)], |r| {
        Ok(LinkedPayment {
            id: r.get(0)?,
            posted_on: r.get(1)?,
            amount_cents: r.get(2)?,
            payee: r.get(3)?,
            from_account_id: r.get(4)?,
            from_account_name: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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
            transfer_account_id: None,
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
    fn linked_transfers_are_not_spending() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let loan = accounts::create(&mut c, &input("Car loan", "loan")).unwrap();
        let mut pay = tx(a.id, "2026-09-05", 300_000, "Payroll");
        pay.category_id = Some(cat(&c, "Income"));
        create(&c, &pay).unwrap();
        let mut car = tx(a.id, "2026-09-06", -45_000, "Car payment");
        car.transfer_account_id = Some(loan.id);
        let car = create(&c, &car).unwrap();
        assert_eq!(car.transfer_account_name.as_deref(), Some("Car loan"));
        create(&c, &tx(a.id, "2026-09-07", -5_000, "Groceries")).unwrap();

        let s = summary(&c, "2026-09-01", "2026-09-30", crate::accounts::Scope::Real).unwrap();
        assert_eq!(s.income_cents, 300_000);
        assert_eq!(s.spending_cents, 5_000, "the loan payment is a transfer, not spending");
        // It still shows in the list, with the link.
        let page = query(&c, &TransactionFilter::default()).unwrap();
        assert_eq!(page.total, 3);

        // A transfer must point somewhere else, and somewhere real.
        let mut self_ref = tx(a.id, "2026-09-08", -100, "Nope");
        self_ref.transfer_account_id = Some(a.id);
        assert!(create(&c, &self_ref).is_err());
        let mut missing = tx(a.id, "2026-09-08", -100, "Nope");
        missing.transfer_account_id = Some(9_999);
        assert!(create(&c, &missing).is_err());

        // Unlinking puts it back in spending.
        let mut edit = tx(a.id, "2026-09-06", -45_000, "Car payment");
        edit.transfer_account_id = None;
        update(&c, car.id, &edit).unwrap();
        let s = summary(&c, "2026-09-01", "2026-09-30", crate::accounts::Scope::Real).unwrap();
        assert_eq!(s.spending_cents, 50_000);
    }

    #[test]
    fn recurring_debt_payments_are_recognised_by_payee() {
        let mut c = conn();
        let checking = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let other = accounts::create(&mut c, &input("Second checking", "checking")).unwrap();
        let loan = accounts::create(&mut c, &input("Car loan", "loan")).unwrap();

        // Tag one payment by hand.
        let mut first = tx(checking.id, "2026-08-05", -70_000, "HONDA FINANCIAL SVC #4821");
        first.transfer_account_id = Some(loan.id);
        create(&c, &first).unwrap();

        // The same payee next month, from the same account, is recognised —
        // reference numbers and case don't matter.
        assert_eq!(
            suggest_transfer_account(&c, checking.id, "honda financial svc #9137").unwrap(),
            Some(loan.id)
        );
        // A different account isn't assumed to have the same arrangement.
        assert_eq!(suggest_transfer_account(&c, other.id, "HONDA FINANCIAL SVC").unwrap(), None);
        // An unrelated payee stays untagged.
        assert_eq!(suggest_transfer_account(&c, checking.id, "Whole Foods").unwrap(), None);

        // A payee that isn't the same name doesn't match: only the reference
        // number is ignored, not a different description.
        assert_eq!(suggest_transfer_account(&c, checking.id, "Honda Financial").unwrap(), None);

        // Re-tagging to somewhere else wins from then on.
        let card = accounts::create(&mut c, &input("Card", "credit_card")).unwrap();
        let mut moved = tx(checking.id, "2026-09-05", -70_000, "Honda Financial Svc");
        moved.transfer_account_id = Some(card.id);
        create(&c, &moved).unwrap();
        assert_eq!(suggest_transfer_account(&c, checking.id, "HONDA FINANCIAL SVC").unwrap(), Some(card.id));
    }

    #[test]
    fn payments_to_a_debt_list_their_source_account() {
        let mut c = conn();
        let checking = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let loan = accounts::create(&mut c, &input("Car loan", "loan")).unwrap();
        for date in ["2026-07-05", "2026-08-05"] {
            let mut p = tx(checking.id, date, -70_000, "Honda Financial");
            p.transfer_account_id = Some(loan.id);
            create(&c, &p).unwrap();
        }
        // Money coming *in* isn't a payment to the debt.
        let mut refund = tx(checking.id, "2026-08-10", 5_000, "Honda refund");
        refund.transfer_account_id = Some(loan.id);
        create(&c, &refund).unwrap();

        let payments = payments_to(&c, loan.id, 6).unwrap();
        assert_eq!(payments.len(), 2);
        assert_eq!(payments[0].posted_on, "2026-08-05", "newest first");
        assert_eq!(payments[0].amount_cents, 70_000, "reported as a positive payment");
        assert_eq!(payments[0].from_account_name, "Checking");
    }

    #[test]
    fn investing_counts_as_saved_not_spent() {
        let mut c = conn();
        let checking = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let brokerage = accounts::create(&mut c, &input("Brokerage", "brokerage")).unwrap();
        let hysa = accounts::create(&mut c, &input("Ally HYSA", "hysa")).unwrap();

        let mut pay = tx(checking.id, "2026-09-01", 600_000, "Payroll");
        pay.category_id = Some(cat(&c, "Income"));
        create(&c, &pay).unwrap();

        // Tagged moves into investments and savings.
        for (target, cents) in [(brokerage.id, -200_000), (hysa.id, -100_000)] {
            let mut moved = tx(checking.id, "2026-09-05", cents, "Transfer");
            moved.transfer_account_id = Some(target);
            create(&c, &moved).unwrap();
        }
        // Real spending, and some money out with no category at all.
        let mut groceries = tx(checking.id, "2026-09-06", -50_000, "Whole Foods");
        groceries.category_id = Some(cat(&c, "Groceries"));
        create(&c, &groceries).unwrap();
        create(&c, &tx(checking.id, "2026-09-07", -30_000, "Who knows")).unwrap();

        let s = summary(&c, "2026-09-01", "2026-09-30", Scope::Real).unwrap();
        assert_eq!(s.income_cents, 600_000);
        assert_eq!(s.saved_cents, 300_000, "both the brokerage and the HYSA count as saving");
        assert_eq!(s.spending_cents, 80_000, "investing isn't spending; the uncategorized $300 is");
        assert_eq!(s.uncategorized_spending_cents, 30_000, "surfaced so a wrong rate can be explained");

        // So the savings rate the dashboard shows is (600 - 80) / 600 = 86%.
        let rate = (s.income_cents - s.spending_cents) as f64 / s.income_cents as f64;
        assert!((rate - 0.866).abs() < 0.01, "rate={rate}");
    }

    #[test]
    fn an_untagged_move_to_investments_looks_like_spending() {
        // Why a savings rate can go deeply negative: money out with no
        // category and no transfer tag is counted as spending, because
        // nothing records where it went.
        let mut c = conn();
        let checking = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        accounts::create(&mut c, &input("Brokerage", "brokerage")).unwrap();
        let mut pay = tx(checking.id, "2026-09-01", 100_000, "Payroll");
        pay.category_id = Some(cat(&c, "Income"));
        create(&c, &pay).unwrap();
        create(&c, &tx(checking.id, "2026-09-05", -1_000_000, "To brokerage")).unwrap();

        let s = summary(&c, "2026-09-01", "2026-09-30", Scope::Real).unwrap();
        assert_eq!(s.saved_cents, 0);
        assert_eq!(s.spending_cents, 1_000_000);
        assert_eq!(s.uncategorized_spending_cents, 1_000_000);
        // (1,000 - 10,000) / 1,000 = -900%: exactly the complaint.
        let rate = (s.income_cents - s.spending_cents) as f64 / s.income_cents as f64;
        assert!((rate + 9.0).abs() < 0.01, "rate={rate}");
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
