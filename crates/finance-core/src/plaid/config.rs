//! Plaid credentials, stored inside the encrypted vault.
//!
//! The client ID and secret live in the vault's settings table, so they are
//! protected by the same AES-256 encryption and master password as the rest
//! of your data, and exist in memory only while the vault is unlocked. The
//! secret is never sent to the UI.

use rand::RngCore;
use rusqlite::Connection;
use serde::Serialize;
use zeroize::Zeroizing;

use super::api::{Environment, HttpPlaid};
use crate::error::{CoreError, Result};
use crate::settings;

const ENV: &str = "plaid.environment";
const CLIENT_ID: &str = "plaid.client_id";
const SECRET: &str = "plaid.secret";
const USER_ID: &str = "plaid.client_user_id";

pub struct Credentials {
    pub environment: Environment,
    pub client_id: String,
    pub secret: Zeroizing<String>,
}

impl Credentials {
    pub fn client(&self) -> HttpPlaid {
        HttpPlaid::new(self.environment, &self.client_id, &self.secret)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaidStatus {
    pub configured: bool,
    pub environment: Option<Environment>,
    /// Last 4 characters of the client ID, for display.
    pub client_id_hint: Option<String>,
}

fn check_key(field: &str, v: &str) -> Result<String> {
    let v = v.trim();
    if v.len() < 16 || v.len() > 64 || !v.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(CoreError::Invalid(format!(
            "That doesn't look like a Plaid {field}. Copy it from the Plaid Dashboard (Developers → Keys)."
        )));
    }
    Ok(v.to_string())
}

pub fn validate(environment: &str, client_id: &str, secret: &str) -> Result<Credentials> {
    let environment = Environment::parse(environment)
        .ok_or_else(|| CoreError::Invalid("Choose Sandbox or Production.".into()))?;
    Ok(Credentials {
        environment,
        client_id: check_key("client ID", client_id)?,
        secret: Zeroizing::new(check_key("secret", secret)?),
    })
}

pub fn save(conn: &Connection, c: &Credentials) -> Result<()> {
    settings::set(conn, ENV, c.environment.as_str())?;
    settings::set(conn, CLIENT_ID, &c.client_id)?;
    settings::set(conn, SECRET, &c.secret)?;
    Ok(())
}

pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM settings WHERE key IN (?1, ?2, ?3)", [ENV, CLIENT_ID, SECRET])?;
    Ok(())
}

pub fn load(conn: &Connection) -> Result<Option<Credentials>> {
    let env = settings::get(conn, ENV)?.and_then(|e| Environment::parse(&e));
    let id = settings::get(conn, CLIENT_ID)?;
    let secret = settings::get(conn, SECRET)?.map(Zeroizing::new);
    Ok(match (env, id, secret) {
        (Some(environment), Some(client_id), Some(secret)) => {
            Some(Credentials { environment, client_id, secret })
        }
        _ => None,
    })
}

pub fn require(conn: &Connection) -> Result<Credentials> {
    load(conn)?.ok_or_else(|| {
        CoreError::Invalid("Add your Plaid keys in Settings before connecting a bank.".into())
    })
}

pub fn status(conn: &Connection) -> Result<PlaidStatus> {
    Ok(match load(conn)? {
        Some(c) => PlaidStatus {
            configured: true,
            environment: Some(c.environment),
            client_id_hint: Some(c.client_id.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect()),
        },
        None => PlaidStatus { configured: false, environment: None, client_id_hint: None },
    })
}

/// A random, stable identifier for this installation, sent to Plaid as
/// `client_user_id`. Contains no personal information.
pub fn client_user_id(conn: &Connection) -> Result<String> {
    if let Some(id) = settings::get(conn, USER_ID)? {
        return Ok(id);
    }
    let mut b = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut b);
    let id = format!("local-{}", hex::encode(b));
    settings::set(conn, USER_ID, &id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::tests::conn;

    #[test]
    fn save_load_status_clear() {
        let c = conn();
        assert!(!status(&c).unwrap().configured);
        assert!(require(&c).is_err());
        assert!(validate("development", "a".repeat(24).as_str(), "b".repeat(30).as_str()).is_err());
        assert!(validate("sandbox", "short", "b".repeat(30).as_str()).is_err());
        assert!(validate("sandbox", &"a".repeat(24), "has spaces in it!!!!!").is_err());
        let creds = validate("sandbox", " 0123456789abcdef01234567 ", &"f".repeat(30)).unwrap();
        save(&c, &creds).unwrap();
        let st = status(&c).unwrap();
        assert!(st.configured);
        assert_eq!(st.environment, Some(Environment::Sandbox));
        assert_eq!(st.client_id_hint.as_deref(), Some("4567"));
        let json = serde_json::to_string(&st).unwrap();
        assert!(!json.contains("ffff"), "secret must never be serialized");
        assert_eq!(load(&c).unwrap().unwrap().secret.as_str(), "f".repeat(30));
        clear(&c).unwrap();
        assert!(!status(&c).unwrap().configured);
    }

    #[test]
    fn user_id_is_stable() {
        let c = conn();
        let a = client_user_id(&c).unwrap();
        assert!(a.starts_with("local-"));
        assert_eq!(a, client_user_id(&c).unwrap());
    }
}
