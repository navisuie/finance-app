//! Recognizing both legs of a transfer between two accounts you track (e.g.
//! checking -> brokerage) so you don't have to tag each side by hand.
//!
//! Matching only *suggests* — see `find_candidates` — nothing links or
//! excludes a transaction from spending until `confirm_link` is called.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::accounts::Scope;
use crate::error::{CoreError, Result};
use crate::transactions::{self, Transaction};

/// A same-day match is highest confidence; this window also covers ACH lag
/// (a debit posts a day or two before the matching credit). Not currently
/// user-tunable — see PFA.md's open question if that's ever needed.
const LOOKBACK_DAYS: i64 = 3;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransferCandidate {
    pub a: Transaction,
    pub b: Transaction,
}

/// Unlinked, undismissed transaction pairs that look like the two sides of a
/// transfer: equal and opposite amounts, different accounts in the same
/// scope, posted within `LOOKBACK_DAYS` of each other. A transaction that
/// matches more than one candidate on either side is left out entirely
/// rather than guessed.
pub fn find_candidates(conn: &Connection, scope: Scope) -> Result<Vec<TransferCandidate>> {
    let sql = format!(
        "WITH pairs AS (
            SELECT t1.id AS a_id, t2.id AS b_id
            FROM transactions t1
            JOIN accounts a ON a.id = t1.account_id
            JOIN transactions t2 ON t2.amount_cents = -t1.amount_cents
                 AND t2.account_id != t1.account_id
                 AND ABS(julianday(t2.posted_on) - julianday(t1.posted_on)) <= {LOOKBACK_DAYS}
            JOIN accounts a2 ON a2.id = t2.account_id AND a2.is_test = a.is_test
            WHERE {scope_sql} AND t1.amount_cents != 0
              AND t1.transfer_account_id IS NULL AND t1.linked_transaction_id IS NULL AND t1.transfer_dismissed = 0
              AND t2.transfer_account_id IS NULL AND t2.linked_transaction_id IS NULL AND t2.transfer_dismissed = 0
              AND t1.id < t2.id
        )
        SELECT a_id, b_id FROM pairs p
        WHERE (SELECT count(*) FROM pairs WHERE a_id = p.a_id) = 1
          AND (SELECT count(*) FROM pairs WHERE b_id = p.b_id) = 1
        ORDER BY a_id DESC",
        scope_sql = scope.sql(),
    );
    let mut stmt = conn.prepare(&sql)?;
    let ids: Vec<(i64, i64)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    ids.into_iter()
        .map(|(a_id, b_id)| Ok(TransferCandidate { a: transactions::get(conn, a_id)?, b: transactions::get(conn, b_id)? }))
        .collect()
}

fn check_unlinked(t: &Transaction) -> Result<()> {
    if t.transfer_account_id.is_some() || t.linked_transaction_id.is_some() {
        return Err(CoreError::Invalid("That transaction is already linked to a transfer.".into()));
    }
    Ok(())
}

/// Confirms a candidate pair: each side gets `transfer_account_id` pointing
/// at the other's account (so existing spend/income/budget exclusion just
/// works) and `linked_transaction_id` pointing at each other.
pub fn confirm_link(conn: &mut Connection, a_id: i64, b_id: i64) -> Result<()> {
    if a_id == b_id {
        return Err(CoreError::Invalid("A transfer has to point at a different transaction.".into()));
    }
    let a = transactions::get(conn, a_id)?;
    let b = transactions::get(conn, b_id)?;
    check_unlinked(&a)?;
    check_unlinked(&b)?;
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE transactions SET transfer_account_id = ?1, linked_transaction_id = ?2 WHERE id = ?3",
        params![b.account_id, b_id, a_id],
    )?;
    tx.execute(
        "UPDATE transactions SET transfer_account_id = ?1, linked_transaction_id = ?2 WHERE id = ?3",
        params![a.account_id, a_id, b_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// Marks a suggested pair as not a transfer so it stops being suggested.
pub fn dismiss_candidate(conn: &Connection, a_id: i64, b_id: i64) -> Result<()> {
    conn.execute("UPDATE transactions SET transfer_dismissed = 1 WHERE id IN (?1, ?2)", params![a_id, b_id])?;
    Ok(())
}

/// Undoes a confirmed link on both legs; each goes back to counting as
/// ordinary spending/income unless re-tagged.
pub fn unlink(conn: &mut Connection, id: i64) -> Result<()> {
    let t = transactions::get(conn, id)?;
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE transactions SET transfer_account_id = NULL, linked_transaction_id = NULL WHERE id = ?1",
        [id],
    )?;
    if let Some(other) = t.linked_transaction_id {
        tx.execute(
            "UPDATE transactions SET transfer_account_id = NULL, linked_transaction_id = NULL WHERE id = ?1",
            [other],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{self, tests::{conn, input}};
    use crate::transactions::tests::tx;

    #[test]
    fn exact_and_near_day_matches() {
        let mut c = conn();
        let checking = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let brokerage = accounts::create(&mut c, &input("Brokerage", "brokerage")).unwrap();
        let out = transactions::create(&c, &tx(checking.id, "2026-09-05", -100_000, "Transfer")).unwrap();
        let in_ = transactions::create(&c, &tx(brokerage.id, "2026-09-06", 100_000, "Transfer in")).unwrap();

        let candidates = find_candidates(&c, Scope::Real).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!((candidates[0].a.id, candidates[0].b.id), (out.id, in_.id));

        confirm_link(&mut c, out.id, in_.id).unwrap();
        let out2 = transactions::get(&c, out.id).unwrap();
        assert_eq!(out2.transfer_account_id, Some(brokerage.id));
        assert_eq!(out2.linked_transaction_id, Some(in_.id));
        let in2 = transactions::get(&c, in_.id).unwrap();
        assert_eq!(in2.transfer_account_id, Some(checking.id));
        assert_eq!(in2.linked_transaction_id, Some(out.id));

        // Now linked, so it no longer shows up as a candidate.
        assert!(find_candidates(&c, Scope::Real).unwrap().is_empty());

        unlink(&mut c, out.id).unwrap();
        assert_eq!(transactions::get(&c, out.id).unwrap().transfer_account_id, None);
        assert_eq!(transactions::get(&c, in_.id).unwrap().linked_transaction_id, None);
        // Unlinked again, so it's back as a candidate.
        assert_eq!(find_candidates(&c, Scope::Real).unwrap().len(), 1);
    }

    #[test]
    fn ambiguous_pairs_are_skipped() {
        let mut c = conn();
        let checking = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let savings = accounts::create(&mut c, &input("HYSA", "hysa")).unwrap();
        transactions::create(&c, &tx(checking.id, "2026-09-05", -50_000, "Out")).unwrap();
        transactions::create(&c, &tx(savings.id, "2026-09-05", 50_000, "In 1")).unwrap();
        transactions::create(&c, &tx(savings.id, "2026-09-06", 50_000, "In 2")).unwrap();

        // Two equally good matches on the other side: left ambiguous, not guessed.
        assert!(find_candidates(&c, Scope::Real).unwrap().is_empty());
    }

    #[test]
    fn cross_scope_and_wrong_sign_never_match() {
        let mut c = conn();
        let real = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let test_acct = accounts::create(&mut c, &input("Sandbox checking", "checking")).unwrap();
        accounts::set_is_test(&c, test_acct.id, true).unwrap();
        transactions::create(&c, &tx(real.id, "2026-09-05", -50_000, "Out")).unwrap();
        transactions::create(&c, &tx(test_acct.id, "2026-09-05", 50_000, "In")).unwrap();

        // Different scopes: never paired even though amounts/dates match.
        assert!(find_candidates(&c, Scope::Real).unwrap().is_empty());
        assert!(find_candidates(&c, Scope::Test).unwrap().is_empty());
        assert!(find_candidates(&c, Scope::All).unwrap().is_empty());

        let other = accounts::create(&mut c, &input("Card", "credit_card")).unwrap();
        // Same sign, not opposite: not a transfer pair.
        transactions::create(&c, &tx(other.id, "2026-09-05", -50_000, "Also out")).unwrap();
        assert!(find_candidates(&c, Scope::Real).unwrap().is_empty());
    }

    #[test]
    fn cannot_link_a_transaction_twice() {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Checking", "checking")).unwrap();
        let b = accounts::create(&mut c, &input("Savings", "hysa")).unwrap();
        let x = transactions::create(&c, &tx(a.id, "2026-09-05", -1_000, "Out")).unwrap();
        let y = transactions::create(&c, &tx(b.id, "2026-09-05", 1_000, "In")).unwrap();
        let z = transactions::create(&c, &tx(b.id, "2026-09-05", 1_000, "In 2")).unwrap();
        confirm_link(&mut c, x.id, y.id).unwrap();
        assert!(confirm_link(&mut c, x.id, z.id).is_err());
    }
}
