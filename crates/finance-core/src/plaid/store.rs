//! Database side of Plaid: items, account links, and applying synced data.
//! Pure database code (no network), so it is fully unit-tested.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::api::{Environment, Holdings, PlaidAccount, PlaidTransaction, SyncPage};
use super::mapping;
use crate::accounts::{self, AccountInput};
use crate::categories;
use crate::error::{CoreError, Result};
use crate::transactions::payee_category_map;
use crate::util::{normalize_payee, now_rfc3339};

pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_NEEDS_REAUTH: &str = "needs_reauth";
pub const STATUS_ERROR: &str = "error";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: i64,
    pub institution_name: String,
    pub status: String,
    pub last_error: Option<String>,
    pub last_synced_at: Option<String>,
    pub products: Vec<String>,
    pub environment: Option<Environment>,
    pub linked_account_count: i64,
    pub created_at: String,
}

/// Secrets and sync state for one item. Never sent to the UI.
pub struct ItemSecret {
    pub id: i64,
    pub access_token: String,
    pub cursor: Option<String>,
    pub products: Vec<String>,
    pub institution_name: String,
}

fn split_products(s: &str) -> Vec<String> {
    s.split(',').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
}

pub fn upsert_item(
    conn: &Connection,
    item_id: &str,
    access_token: &str,
    institution_id: Option<&str>,
    institution_name: Option<&str>,
    products: &[String],
) -> Result<i64> {
    conn.execute(
        "INSERT INTO plaid_items (item_id, access_token, institution_id, institution_name, products, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6)
         ON CONFLICT(item_id) DO UPDATE SET access_token = excluded.access_token,
             institution_id = coalesce(excluded.institution_id, plaid_items.institution_id),
             institution_name = coalesce(excluded.institution_name, plaid_items.institution_name),
             status = 'active', last_error = NULL",
        params![item_id, access_token, institution_id, institution_name, products.join(","), now_rfc3339()],
    )?;
    Ok(conn.query_row("SELECT id FROM plaid_items WHERE item_id = ?1", [item_id], |r| r.get(0))?)
}

pub fn list_items(conn: &Connection) -> Result<Vec<ItemView>> {
    let mut stmt = conn.prepare(
        "SELECT i.id, coalesce(i.institution_name, i.institution_id, 'Bank'), i.status, i.last_error,
                i.last_synced_at, i.products, i.access_token, i.created_at,
                (SELECT count(*) FROM accounts a WHERE a.plaid_item_id = i.id)
         FROM plaid_items i ORDER BY i.institution_name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let token: String = r.get(6)?;
        let products: String = r.get(5)?;
        Ok(ItemView {
            id: r.get(0)?,
            institution_name: r.get(1)?,
            status: r.get(2)?,
            last_error: r.get(3)?,
            last_synced_at: r.get(4)?,
            products: split_products(&products),
            environment: Environment::from_access_token(&token),
            linked_account_count: r.get(8)?,
            created_at: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn item_secret(conn: &Connection, id: i64) -> Result<ItemSecret> {
    conn.query_row(
        "SELECT id, access_token, tx_cursor, products, coalesce(institution_name, 'Bank')
         FROM plaid_items WHERE id = ?1",
        [id],
        |r| {
            let products: String = r.get(3)?;
            Ok(ItemSecret {
                id: r.get(0)?,
                access_token: r.get(1)?,
                cursor: r.get(2)?,
                products: split_products(&products),
                institution_name: r.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("That bank connection no longer exists.".into()))
}

pub fn set_status(conn: &Connection, id: i64, status: &str, error: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE plaid_items SET status = ?1, last_error = ?2 WHERE id = ?3",
        params![status, error, id],
    )?;
    Ok(())
}

/// Forget an item locally. Its accounts and their history are kept but
/// become manual accounts.
pub fn delete_item(conn: &mut Connection, id: i64) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE accounts SET plaid_item_id = NULL, plaid_account_id = NULL, plaid_sync_from = NULL
         WHERE plaid_item_id = ?1",
        [id],
    )?;
    tx.execute("DELETE FROM plaid_items WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

/// Plaid account id -> (local account id, sync-from date) for an item.
pub fn linked_accounts(conn: &Connection, item_db_id: i64) -> Result<HashMap<String, (i64, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT plaid_account_id, id, plaid_sync_from FROM accounts
         WHERE plaid_item_id = ?1 AND plaid_account_id IS NOT NULL",
    )?;
    let rows = stmt.query_map([item_db_id], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?))))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ------------------------------------------------------------ account links

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkAction {
    Create,
    Link,
    Skip,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountChoice {
    pub plaid_account_id: String,
    pub action: LinkAction,
    /// For `Link`: the existing account.
    pub account_id: Option<i64>,
    /// For `Create`: account kind (defaults from Plaid's type).
    pub kind: Option<String>,
    /// For `Create`: display name (defaults from Plaid).
    pub name: Option<String>,
    /// For `Link`: ignore Plaid transactions before this date.
    pub sync_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaidAccountView {
    pub plaid_account_id: String,
    pub name: String,
    pub official_name: Option<String>,
    pub mask: Option<String>,
    pub account_type: String,
    pub subtype: Option<String>,
    pub suggested_kind: String,
    pub balance_cents: Option<i64>,
    /// Local account currently linked to this Plaid account.
    pub linked_account_id: Option<i64>,
    /// An unlinked local account that looks like the same account.
    pub suggested_match_id: Option<i64>,
    /// For a suggested/linked match: day after its latest transaction.
    pub suggested_sync_from: Option<String>,
}

fn clean_mask(mask: Option<&str>) -> Option<String> {
    let m: String = mask?.chars().filter(|c| c.is_ascii_digit()).collect();
    if m.is_empty() {
        return None;
    }
    let start = m.len().saturating_sub(4);
    Some(m[start..].to_string())
}

fn day_after_latest_transaction(conn: &Connection, account_id: i64) -> Result<Option<String>> {
    let latest: Option<String> = conn.query_row(
        "SELECT max(posted_on) FROM transactions WHERE account_id = ?1 AND source != 'plaid'",
        [account_id],
        |r| r.get(0),
    )?;
    Ok(latest
        .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .and_then(|d| d.succ_opt())
        .map(|d| d.format("%Y-%m-%d").to_string()))
}

/// True when this item was created with Sandbox keys, so everything it
/// brings in is test data.
pub fn item_is_test(conn: &Connection, item_db_id: i64) -> Result<bool> {
    let token = item_secret(conn, item_db_id)?.access_token;
    Ok(Environment::from_access_token(&token) == Some(Environment::Sandbox))
}

pub fn account_views(conn: &Connection, item_db_id: i64, plaid: &[PlaidAccount]) -> Result<Vec<PlaidAccountView>> {
    let linked = linked_accounts(conn, item_db_id)?;
    let is_test = item_is_test(conn, item_db_id)?;
    // Only offer to link accounts of the same kind of data: test connections
    // to test accounts, real connections to real accounts.
    let locals: Vec<accounts::Account> = accounts::list(conn, false, accounts::Scope::All)?
        .into_iter()
        .filter(|a| a.is_test == is_test)
        .collect();
    let mut out = Vec::new();
    let mut claimed: HashSet<i64> = HashSet::new();
    for p in plaid {
        let kind = mapping::account_kind(&p.account_type, p.subtype.as_deref()).to_string();
        let mask = clean_mask(p.mask.as_deref());
        let linked_id = linked.get(&p.account_id).map(|(id, _)| *id);
        let suggested = if linked_id.is_some() {
            None
        } else {
            locals
                .iter()
                .filter(|a| !a.linked && !claimed.contains(&a.id))
                .find(|a| mask.is_some() && a.last4 == mask && accounts::default_is_asset(&a.kind) == accounts::default_is_asset(&kind))
                .or_else(|| {
                    locals.iter().filter(|a| !a.linked && !claimed.contains(&a.id)).find(|a| {
                        a.kind == kind && a.name.eq_ignore_ascii_case(&p.name)
                    })
                })
                .map(|a| a.id)
        };
        if let Some(id) = suggested {
            claimed.insert(id);
        }
        let sync_from = match suggested.or(linked_id) {
            Some(id) => day_after_latest_transaction(conn, id)?,
            None => None,
        };
        let bal = p.balances.current.or(p.balances.available).and_then(mapping::balance_cents);
        out.push(PlaidAccountView {
            plaid_account_id: p.account_id.clone(),
            name: p.name.clone(),
            official_name: p.official_name.clone(),
            mask,
            account_type: p.account_type.clone(),
            subtype: p.subtype.clone(),
            suggested_kind: kind,
            balance_cents: bal,
            linked_account_id: linked_id,
            suggested_match_id: suggested,
            suggested_sync_from: sync_from,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinkSummary {
    pub created: usize,
    pub linked: usize,
    pub unlinked: usize,
}

pub fn apply_account_choices(
    conn: &mut Connection,
    item_db_id: i64,
    plaid: &[PlaidAccount],
    choices: &[AccountChoice],
) -> Result<LinkSummary> {
    let item = item_secret(conn, item_db_id)?;
    let is_test = item_is_test(conn, item_db_id)?;
    let by_id: HashMap<&str, &PlaidAccount> = plaid.iter().map(|a| (a.account_id.as_str(), a)).collect();
    let already = linked_accounts(conn, item_db_id)?;
    let mut summary = LinkSummary::default();
    let mut newly_linked = false;
    let mut targets = HashSet::new();

    let tx = conn.transaction()?;
    for c in choices {
        let p = by_id
            .get(c.plaid_account_id.as_str())
            .ok_or_else(|| CoreError::Invalid("Unknown bank account in the request.".into()))?;
        let current = already.get(&c.plaid_account_id).map(|(id, _)| *id);
        match c.action {
            LinkAction::Skip => {
                if let Some(id) = current {
                    tx.execute(
                        "UPDATE accounts SET plaid_item_id = NULL, plaid_account_id = NULL, plaid_sync_from = NULL WHERE id = ?1",
                        [id],
                    )?;
                    summary.unlinked += 1;
                }
            }
            LinkAction::Create => {
                if current.is_some() {
                    continue; // already tracked
                }
                let kind = c
                    .kind
                    .clone()
                    .unwrap_or_else(|| mapping::account_kind(&p.account_type, p.subtype.as_deref()).into());
                let name = c.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| p.name.clone());
                let input = AccountInput {
                    name,
                    kind,
                    institution: Some(item.institution_name.clone()),
                    last4: clean_mask(p.mask.as_deref()),
                    is_asset: None,
                    balance_cents: None,
                    balance_as_of: None,
                    apr_bps: None,
                    monthly_payment_cents: None,
                };
                let created = accounts::insert(&tx, &input)?;
                tx.execute(
                    "UPDATE accounts SET plaid_item_id = ?1, plaid_account_id = ?2, is_test = ?3 WHERE id = ?4",
                    params![item_db_id, p.account_id, is_test, created],
                )?;
                summary.created += 1;
                newly_linked = true;
            }
            LinkAction::Link => {
                let target = c
                    .account_id
                    .ok_or_else(|| CoreError::Invalid("Choose which account to link.".into()))?;
                if !targets.insert(target) {
                    return Err(CoreError::Invalid("Two bank accounts can't link to the same account.".into()));
                }
                if current == Some(target) {
                    continue;
                }
                let acct = accounts::get(&tx, target)?;
                if acct.linked {
                    return Err(CoreError::Invalid(format!("{} is already connected to a bank.", acct.name)));
                }
                if acct.is_test != is_test {
                    return Err(CoreError::Invalid(if is_test {
                        format!("{} holds real data, so a Sandbox test connection can't be linked to it.", acct.name)
                    } else {
                        format!("{} is a test account, so a real bank can't be linked to it.", acct.name)
                    }));
                }
                if let Some(old) = current {
                    tx.execute(
                        "UPDATE accounts SET plaid_item_id = NULL, plaid_account_id = NULL, plaid_sync_from = NULL WHERE id = ?1",
                        [old],
                    )?;
                }
                let sync_from = match &c.sync_from {
                    Some(d) if !d.is_empty() => Some(crate::util::parse_date(d)?),
                    _ => None,
                };
                tx.execute(
                    "UPDATE accounts SET plaid_item_id = ?1, plaid_account_id = ?2, plaid_sync_from = ?3 WHERE id = ?4",
                    params![item_db_id, p.account_id, sync_from, target],
                )?;
                summary.linked += 1;
                newly_linked = true;
            }
        }
    }
    // A newly tracked account needs history the old cursor already passed.
    if newly_linked && item.cursor.is_some() {
        tx.execute("UPDATE plaid_items SET tx_cursor = NULL WHERE id = ?1", [item_db_id])?;
    }
    tx.commit()?;
    Ok(summary)
}

// ------------------------------------------------------------- data apply

pub fn apply_balances(conn: &Connection, item_db_id: i64, plaid: &[PlaidAccount], today: &str) -> Result<usize> {
    let linked = linked_accounts(conn, item_db_id)?;
    let mut n = 0;
    for p in plaid {
        let Some((account_id, _)) = linked.get(&p.account_id) else { continue };
        let Some(cents) = p.balances.current.or(p.balances.available).and_then(mapping::balance_cents) else {
            continue;
        };
        conn.execute(
            "INSERT INTO balances (account_id, as_of, amount_cents, source) VALUES (?1, ?2, ?3, 'plaid')
             ON CONFLICT(account_id, as_of) DO UPDATE SET amount_cents = excluded.amount_cents, source = 'plaid'",
            params![account_id, today, cents],
        )?;
        n += 1;
    }
    Ok(n)
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncCounts {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub skipped: usize,
}

fn payee_of(t: &PlaidTransaction) -> String {
    let raw = t
        .merchant_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(t.name.as_deref())
        .unwrap_or("Unknown");
    let p: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let p: String = p.chars().take(200).collect();
    if p.is_empty() { "Unknown".into() } else { p }
}

fn valid_date(d: &str) -> Option<String> {
    NaiveDate::parse_from_str(d, "%Y-%m-%d").ok().map(|d| d.format("%Y-%m-%d").to_string())
}

/// Apply all pages of one /transactions/sync run atomically and store the
/// final cursor.
pub fn apply_sync(conn: &mut Connection, item_db_id: i64, pages: &[SyncPage]) -> Result<SyncCounts> {
    let linked = linked_accounts(conn, item_db_id)?;
    let learned = payee_category_map(conn)?;
    let cat_ids: HashMap<String, i64> =
        categories::list(conn)?.into_iter().map(|c| (c.name.to_lowercase(), c.id)).collect();
    let now = now_rfc3339();
    let mut counts = SyncCounts::default();

    let tx = conn.transaction()?;
    {
        let mut find_cat = tx.prepare(
            "SELECT category_id FROM transactions WHERE plaid_transaction_id = ?1",
        )?;
        let mut upsert = tx.prepare(
            "INSERT INTO transactions (account_id, posted_on, amount_cents, payee, category_id, pending,
                                       source, plaid_transaction_id, plaid_category, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'plaid', ?7, ?8, ?9)
             ON CONFLICT(plaid_transaction_id) DO UPDATE SET
                account_id = excluded.account_id, posted_on = excluded.posted_on,
                amount_cents = excluded.amount_cents, payee = excluded.payee,
                pending = excluded.pending, plaid_category = excluded.plaid_category",
        )?;
        let mut delete = tx.prepare("DELETE FROM transactions WHERE plaid_transaction_id = ?1")?;

        for page in pages {
            for (is_new, t) in page.added.iter().map(|t| (true, t)).chain(page.modified.iter().map(|t| (false, t))) {
                let Some((account_id, sync_from)) = linked.get(&t.account_id) else {
                    counts.skipped += 1;
                    continue;
                };
                let (Some(date), Some(cents)) = (valid_date(&t.date), mapping::to_cents(t.amount)) else {
                    counts.skipped += 1;
                    continue;
                };
                if sync_from.as_deref().is_some_and(|f| date.as_str() < f) {
                    counts.skipped += 1;
                    continue;
                }
                let payee = payee_of(t);
                let pfc = t.personal_finance_category.as_ref();
                // Category priority: the pending version's category (keeps the
                // user's choice when a charge posts), then what the user used
                // for this payee before, then Plaid's category.
                let from_pending: Option<i64> = match &t.pending_transaction_id {
                    Some(pid) => find_cat.query_row([pid], |r| r.get(0)).optional()?.flatten(),
                    None => None,
                };
                let category = from_pending
                    .or_else(|| learned.get(&normalize_payee(&payee)).copied())
                    .or_else(|| {
                        pfc.and_then(|c| mapping::category_name(&c.primary, c.detailed.as_deref()))
                            .and_then(|name| cat_ids.get(&name.to_lowercase()).copied())
                    });
                let detailed = pfc.map(|c| c.detailed.clone().unwrap_or_else(|| c.primary.clone()));
                upsert.execute(params![
                    account_id, date, cents, payee, category, t.pending, t.transaction_id, detailed, now
                ])?;
                if is_new { counts.added += 1 } else { counts.modified += 1 }
            }
            for r in &page.removed {
                counts.removed += delete.execute([&r.transaction_id])?;
            }
        }
    }
    if let Some(last) = pages.last() {
        tx.execute(
            "UPDATE plaid_items SET tx_cursor = ?1 WHERE id = ?2",
            params![last.next_cursor, item_db_id],
        )?;
    }
    tx.commit()?;
    Ok(counts)
}

pub fn apply_holdings(conn: &mut Connection, item_db_id: i64, h: &Holdings, today: &str) -> Result<usize> {
    let linked = linked_accounts(conn, item_db_id)?;
    let tx = conn.transaction()?;
    let mut n = 0;
    {
        let mut sec_ids: HashMap<&str, i64> = HashMap::new();
        for s in &h.securities {
            tx.execute(
                "INSERT INTO securities (plaid_security_id, ticker, name, kind) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(plaid_security_id) DO UPDATE SET ticker = excluded.ticker,
                     name = excluded.name, kind = excluded.kind",
                params![
                    s.security_id,
                    s.ticker_symbol,
                    s.name.clone().unwrap_or_else(|| s.ticker_symbol.clone().unwrap_or_else(|| "Unknown".into())),
                    s.security_type
                ],
            )?;
            let id: i64 = tx.query_row(
                "SELECT id FROM securities WHERE plaid_security_id = ?1",
                [&s.security_id],
                |r| r.get(0),
            )?;
            sec_ids.insert(s.security_id.as_str(), id);
        }
        // Replace holdings for every linked account in the response.
        let in_response: HashSet<&str> = h
            .accounts
            .iter()
            .map(|a| a.account_id.as_str())
            .chain(h.holdings.iter().map(|x| x.account_id.as_str()))
            .collect();
        for pid in in_response {
            if let Some((account_id, _)) = linked.get(pid) {
                tx.execute("DELETE FROM holdings WHERE account_id = ?1", [account_id])?;
            }
        }
        for hold in &h.holdings {
            let (Some((account_id, _)), Some(sec)) =
                (linked.get(&hold.account_id), sec_ids.get(hold.security_id.as_str()))
            else {
                continue;
            };
            if !hold.quantity.is_finite() {
                continue;
            }
            let value = hold
                .institution_value
                .or_else(|| hold.institution_price.map(|p| p * hold.quantity))
                .and_then(mapping::balance_cents)
                .unwrap_or(0);
            let basis = hold.cost_basis.and_then(mapping::balance_cents);
            tx.execute(
                "INSERT INTO holdings (account_id, security_id, quantity, cost_basis_cents, value_cents, as_of)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(account_id, security_id) DO UPDATE SET quantity = quantity + excluded.quantity,
                     cost_basis_cents = coalesce(cost_basis_cents, 0) + coalesce(excluded.cost_basis_cents, 0),
                     value_cents = value_cents + excluded.value_cents",
                params![account_id, sec, hold.quantity, basis, value, today],
            )?;
            n += 1;
        }
    }
    tx.commit()?;
    Ok(n)
}

pub fn mark_synced(conn: &Connection, item_db_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE plaid_items SET last_synced_at = ?1, status = 'active', last_error = NULL WHERE id = ?2",
        params![now_rfc3339(), item_db_id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HoldingView {
    pub ticker: Option<String>,
    pub name: String,
    pub security_type: Option<String>,
    pub quantity: f64,
    pub value_cents: i64,
    pub cost_basis_cents: Option<i64>,
    pub as_of: String,
}

pub fn list_holdings(conn: &Connection, account_id: i64) -> Result<Vec<HoldingView>> {
    let mut stmt = conn.prepare(
        "SELECT s.ticker, s.name, s.kind, h.quantity, h.value_cents, h.cost_basis_cents, h.as_of
         FROM holdings h JOIN securities s ON s.id = h.security_id
         WHERE h.account_id = ?1 ORDER BY h.value_cents DESC",
    )?;
    let rows = stmt.query_map([account_id], |r| {
        Ok(HoldingView {
            ticker: r.get(0)?,
            name: r.get(1)?,
            security_type: r.get(2)?,
            quantity: r.get(3)?,
            value_cents: r.get(4)?,
            cost_basis_cents: r.get(5)?,
            as_of: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
