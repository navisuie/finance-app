//! Schema and migrations. Versioned with `PRAGMA user_version`.
//!
//! All money columns are integer cents. Timestamps are RFC 3339 UTC strings;
//! calendar dates are `YYYY-MM-DD`.

use rusqlite::Connection;

use crate::error::{CoreError, Result};

const MIGRATIONS: &[&str] = &[
    // v1: full MVP schema, including Plaid tables used in phase 6.
    r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE plaid_items (
        id               INTEGER PRIMARY KEY,
        item_id          TEXT NOT NULL UNIQUE,
        access_token     TEXT NOT NULL,          -- stored only inside this encrypted DB
        institution_id   TEXT,
        institution_name TEXT,
        tx_cursor        TEXT,                   -- /transactions/sync cursor
        status           TEXT NOT NULL DEFAULT 'active', -- active | needs_reauth | removed
        last_synced_at   TEXT,
        created_at       TEXT NOT NULL
    );

    CREATE TABLE accounts (
        id               INTEGER PRIMARY KEY,
        name             TEXT NOT NULL,
        kind             TEXT NOT NULL CHECK (kind IN
                           ('checking','savings','hysa','brokerage','401k','roth_ira',
                            'traditional_ira','hsa','credit_card','loan','mortgage','other')),
        institution      TEXT,
        last4            TEXT CHECK (last4 IS NULL OR length(last4) <= 4),
        is_asset         INTEGER NOT NULL DEFAULT 1,
        currency         TEXT NOT NULL DEFAULT 'USD',
        archived         INTEGER NOT NULL DEFAULT 0,
        plaid_item_id    INTEGER REFERENCES plaid_items(id) ON DELETE SET NULL,
        plaid_account_id TEXT UNIQUE,
        created_at       TEXT NOT NULL
    );

    CREATE TABLE balances (
        id           INTEGER PRIMARY KEY,
        account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        as_of        TEXT NOT NULL,              -- YYYY-MM-DD
        amount_cents INTEGER NOT NULL,
        source       TEXT NOT NULL DEFAULT 'manual', -- manual | csv | plaid
        UNIQUE (account_id, as_of)
    );

    CREATE TABLE categories (
        id        INTEGER PRIMARY KEY,
        name      TEXT NOT NULL,
        parent_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
        kind      TEXT NOT NULL CHECK (kind IN ('income','expense','transfer')),
        UNIQUE (name, parent_id)
    );

    CREATE TABLE transactions (
        id                   INTEGER PRIMARY KEY,
        account_id           INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        posted_on            TEXT NOT NULL,      -- YYYY-MM-DD
        amount_cents         INTEGER NOT NULL,   -- negative = money out
        payee                TEXT NOT NULL,
        category_id          INTEGER REFERENCES categories(id) ON DELETE SET NULL,
        notes                TEXT,
        pending              INTEGER NOT NULL DEFAULT 0,
        source               TEXT NOT NULL DEFAULT 'manual',
        import_hash          TEXT,
        plaid_transaction_id TEXT UNIQUE,
        created_at           TEXT NOT NULL
    );
    CREATE INDEX idx_tx_account_date ON transactions(account_id, posted_on);
    CREATE UNIQUE INDEX idx_tx_import_hash ON transactions(account_id, import_hash)
        WHERE import_hash IS NOT NULL;

    CREATE TABLE budgets (
        category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
        month       TEXT NOT NULL,               -- YYYY-MM
        limit_cents INTEGER NOT NULL CHECK (limit_cents >= 0),
        PRIMARY KEY (category_id, month)
    );

    CREATE TABLE goals (
        id           INTEGER PRIMARY KEY,
        name         TEXT NOT NULL,
        target_cents INTEGER NOT NULL CHECK (target_cents > 0),
        target_date  TEXT,
        account_id   INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
        created_at   TEXT NOT NULL
    );

    CREATE TABLE scenarios (
        id          INTEGER PRIMARY KEY,
        calculator  TEXT NOT NULL,
        name        TEXT NOT NULL,
        inputs_json TEXT NOT NULL,
        created_at  TEXT NOT NULL
    );

    CREATE TABLE import_profiles (
        id                  INTEGER PRIMARY KEY,
        institution         TEXT NOT NULL UNIQUE,
        column_mapping_json TEXT NOT NULL
    );

    CREATE TABLE securities (
        id               INTEGER PRIMARY KEY,
        plaid_security_id TEXT UNIQUE,
        ticker           TEXT,
        name             TEXT NOT NULL,
        kind             TEXT                    -- equity | etf | mutual fund | cash | ...
    );

    CREATE TABLE holdings (
        account_id         INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        security_id        INTEGER NOT NULL REFERENCES securities(id) ON DELETE CASCADE,
        quantity           REAL NOT NULL,        -- shares can be fractional
        cost_basis_cents   INTEGER,
        value_cents        INTEGER NOT NULL,
        as_of              TEXT NOT NULL,
        PRIMARY KEY (account_id, security_id)
    );

    CREATE TABLE contributions (
        id           INTEGER PRIMARY KEY,
        account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        tax_year     INTEGER NOT NULL,
        amount_cents INTEGER NOT NULL,
        contributed_on TEXT NOT NULL
    );

    INSERT INTO categories (name, kind) VALUES
        ('Income','income'), ('Housing','expense'), ('Utilities','expense'),
        ('Groceries','expense'), ('Dining','expense'), ('Transportation','expense'),
        ('Health','expense'), ('Insurance','expense'), ('Shopping','expense'),
        ('Entertainment','expense'), ('Subscriptions','expense'), ('Travel','expense'),
        ('Education','expense'), ('Gifts & Donations','expense'), ('Fees','expense'),
        ('Taxes','expense'), ('Savings & Investing','transfer'), ('Transfer','transfer'),
        ('Other','expense');
    "#,
    // v2: Plaid sync bookkeeping.
    r#"
    ALTER TABLE plaid_items ADD COLUMN products TEXT NOT NULL DEFAULT 'transactions';
    ALTER TABLE plaid_items ADD COLUMN last_error TEXT;
    -- Ignore Plaid transactions before this date (avoids duplicating history
    -- that was already entered by hand or imported from CSV).
    ALTER TABLE accounts ADD COLUMN plaid_sync_from TEXT;
    -- Plaid's detailed category, kept for reference.
    ALTER TABLE transactions ADD COLUMN plaid_category TEXT;
    "#,
    // v3: keep Plaid Sandbox (test) accounts out of the real numbers.
    r#"
    ALTER TABLE accounts ADD COLUMN is_test INTEGER NOT NULL DEFAULT 0;
    UPDATE accounts SET is_test = 1 WHERE plaid_item_id IN (
        SELECT id FROM plaid_items WHERE access_token LIKE 'access-sandbox-%'
    );
    "#,
];

pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current = schema_version(conn)?;
    let latest = MIGRATIONS.len() as i64;
    if current > latest {
        return Err(CoreError::CorruptVault);
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_database_forward() {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute_batch(MIGRATIONS[0]).unwrap();
        tx.pragma_update(None, "user_version", 1).unwrap();
        tx.commit().unwrap();
        conn.execute(
            "INSERT INTO accounts (name, kind, created_at) VALUES ('Old', 'checking', 'now')",
            [],
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), MIGRATIONS.len() as i64);
        let from: Option<String> = conn
            .query_row("SELECT plaid_sync_from FROM accounts WHERE name = 'Old'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(from, None);
    }

    #[test]
    fn migrates_fresh_db_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), MIGRATIONS.len() as i64);
        migrate(&mut conn).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM categories", [], |r| r.get(0)).unwrap();
        assert!(n > 10);
    }

    #[test]
    fn rejects_bad_account_kind() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let err = conn.execute(
            "INSERT INTO accounts (name, kind, created_at) VALUES ('x', 'crypto_casino', 'now')",
            [],
        );
        assert!(err.is_err());
    }
}
