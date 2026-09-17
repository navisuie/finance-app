//! Key/value app settings stored inside the encrypted database.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

pub const AUTO_LOCK_MINUTES: &str = "auto_lock_minutes";
pub const DEFAULT_AUTO_LOCK_MINUTES: u32 = 5;
pub const MAX_AUTO_LOCK_MINUTES: u32 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub auto_lock_minutes: u32,
}

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

pub fn load(conn: &Connection) -> Result<AppSettings> {
    let auto_lock_minutes = get(conn, AUTO_LOCK_MINUTES)?
        .and_then(|v| v.parse().ok())
        .filter(|m| (1..=MAX_AUTO_LOCK_MINUTES).contains(m))
        .unwrap_or(DEFAULT_AUTO_LOCK_MINUTES);
    Ok(AppSettings { auto_lock_minutes })
}

pub fn set_auto_lock_minutes(conn: &Connection, minutes: u32) -> Result<()> {
    if !(1..=MAX_AUTO_LOCK_MINUTES).contains(&minutes) {
        return Err(CoreError::Invalid(format!(
            "Auto-lock must be between 1 and {MAX_AUTO_LOCK_MINUTES} minutes."
        )));
    }
    set(conn, AUTO_LOCK_MINUTES, &minutes.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;

    #[test]
    fn defaults_and_updates() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(load(&conn).unwrap().auto_lock_minutes, 5);
        set_auto_lock_minutes(&conn, 10).unwrap();
        assert_eq!(load(&conn).unwrap().auto_lock_minutes, 10);
        assert!(set_auto_lock_minutes(&conn, 0).is_err());
        assert!(set_auto_lock_minutes(&conn, 61).is_err());
        set(&conn, AUTO_LOCK_MINUTES, "garbage").unwrap();
        assert_eq!(load(&conn).unwrap().auto_lock_minutes, 5);
    }
}
