//! Plaid credentials, stored inside the encrypted vault.
//!
//! Sandbox and Production keys are kept in separate slots, and one
//! environment is active at a time, so switching between test banks and real
//! banks is a toggle rather than re-typing keys.
//!
//! Everything lives in the vault's settings table, protected by the same
//! AES-256 encryption and master password as the rest of your data, and is in
//! memory only while the vault is unlocked. Secrets are never sent to the UI.

use rand::RngCore;
use rusqlite::Connection;
use serde::Serialize;
use zeroize::Zeroizing;

use super::api::{Environment, HttpPlaid};
use crate::error::{CoreError, Result};
use crate::settings;

const ACTIVE: &str = "plaid.environment";
const USER_ID: &str = "plaid.client_user_id";
/// Keys from the first release, when only one environment was stored.
const LEGACY_CLIENT_ID: &str = "plaid.client_id";
const LEGACY_SECRET: &str = "plaid.secret";

fn client_id_key(env: Environment) -> String {
    format!("plaid.{}.client_id", env.as_str())
}

fn secret_key(env: Environment) -> String {
    format!("plaid.{}.secret", env.as_str())
}

pub struct Credentials {
    pub environment: Environment,
    pub client_id: String,
    pub secret: Zeroizing<String>,
}

/// Redacted on purpose: credentials must never reach a log or a panic message.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("environment", &self.environment)
            .field("client_id", &format_args!("…{}", hint(&self.client_id)))
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl Credentials {
    pub fn client(&self) -> HttpPlaid {
        HttpPlaid::new(self.environment, &self.client_id, &self.secret)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStatus {
    pub environment: Environment,
    pub configured: bool,
    /// Last 4 characters of the client ID, for display.
    pub client_id_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaidStatus {
    /// True when the active environment has keys.
    pub configured: bool,
    pub environment: Environment,
    pub client_id_hint: Option<String>,
    pub environments: Vec<EnvironmentStatus>,
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

pub fn parse_environment(s: &str) -> Result<Environment> {
    Environment::parse(s).ok_or_else(|| CoreError::Invalid("Choose Sandbox or Production.".into()))
}

pub fn validate(environment: &str, client_id: &str, secret: &str) -> Result<Credentials> {
    Ok(Credentials {
        environment: parse_environment(environment)?,
        client_id: check_key("client ID", client_id)?,
        secret: Zeroizing::new(check_key("secret", secret)?),
    })
}

/// Move keys written by the single-environment version into the slot for
/// whichever environment was selected then.
fn migrate_legacy(conn: &Connection) -> Result<()> {
    let (Some(client_id), Some(secret)) =
        (settings::get(conn, LEGACY_CLIENT_ID)?, settings::get(conn, LEGACY_SECRET)?)
    else {
        return Ok(());
    };
    let env = settings::get(conn, ACTIVE)?
        .and_then(|e| Environment::parse(&e))
        .unwrap_or(Environment::Sandbox);
    if settings::get(conn, &client_id_key(env))?.is_none() {
        settings::set(conn, &client_id_key(env), &client_id)?;
        settings::set(conn, &secret_key(env), &secret)?;
    }
    settings::set(conn, ACTIVE, env.as_str())?;
    conn.execute("DELETE FROM settings WHERE key IN (?1, ?2)", [LEGACY_CLIENT_ID, LEGACY_SECRET])?;
    Ok(())
}

/// The environment the app is currently working in (Sandbox by default).
pub fn active_environment(conn: &Connection) -> Result<Environment> {
    migrate_legacy(conn)?;
    Ok(settings::get(conn, ACTIVE)?
        .and_then(|e| Environment::parse(&e))
        .unwrap_or(Environment::Sandbox))
}

pub fn load_for(conn: &Connection, env: Environment) -> Result<Option<Credentials>> {
    migrate_legacy(conn)?;
    let id = settings::get(conn, &client_id_key(env))?;
    let secret = settings::get(conn, &secret_key(env))?.map(Zeroizing::new);
    Ok(match (id, secret) {
        (Some(client_id), Some(secret)) => Some(Credentials { environment: env, client_id, secret }),
        _ => None,
    })
}

/// Credentials for the active environment.
pub fn load(conn: &Connection) -> Result<Option<Credentials>> {
    load_for(conn, active_environment(conn)?)
}

pub fn require(conn: &Connection) -> Result<Credentials> {
    let env = active_environment(conn)?;
    load_for(conn, env)?.ok_or_else(|| {
        CoreError::Invalid(format!(
            "Add your Plaid {} keys in Settings before connecting a bank.",
            env.as_str()
        ))
    })
}

/// Store keys for their environment and make that environment active.
pub fn save(conn: &Connection, c: &Credentials) -> Result<()> {
    migrate_legacy(conn)?;
    settings::set(conn, &client_id_key(c.environment), &c.client_id)?;
    settings::set(conn, &secret_key(c.environment), &c.secret)?;
    settings::set(conn, ACTIVE, c.environment.as_str())?;
    Ok(())
}

/// Switch which environment the app uses. Connections made in the other
/// environment stay stored; they just aren't synced until you switch back.
pub fn set_active_environment(conn: &Connection, env: Environment) -> Result<PlaidStatus> {
    migrate_legacy(conn)?;
    settings::set(conn, ACTIVE, env.as_str())?;
    status(conn)
}

/// Forget the keys for one environment, or for both when `env` is `None`.
pub fn clear(conn: &Connection, env: Option<Environment>) -> Result<()> {
    migrate_legacy(conn)?;
    let envs = match env {
        Some(e) => vec![e],
        None => vec![Environment::Sandbox, Environment::Production],
    };
    for e in envs {
        conn.execute(
            "DELETE FROM settings WHERE key IN (?1, ?2)",
            [client_id_key(e), secret_key(e)],
        )?;
    }
    Ok(())
}

fn hint(client_id: &str) -> String {
    let start = client_id.len().saturating_sub(4);
    client_id[start..].to_string()
}

pub fn status(conn: &Connection) -> Result<PlaidStatus> {
    let active = active_environment(conn)?;
    let mut environments = Vec::new();
    for env in [Environment::Sandbox, Environment::Production] {
        let creds = load_for(conn, env)?;
        environments.push(EnvironmentStatus {
            environment: env,
            configured: creds.is_some(),
            client_id_hint: creds.map(|c| hint(&c.client_id)),
        });
    }
    let current = environments.iter().find(|e| e.environment == active);
    Ok(PlaidStatus {
        configured: current.is_some_and(|e| e.configured),
        environment: active,
        client_id_hint: current.and_then(|e| e.client_id_hint.clone()),
        environments,
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

    const ID_A: &str = "0123456789abcdef01234567";
    const ID_B: &str = "89abcdef0123456789abcdef";

    fn save_env(c: &Connection, env: &str, id: &str) {
        let creds = validate(env, id, &"f".repeat(30)).unwrap();
        save(c, &creds).unwrap();
    }

    #[test]
    fn keeps_both_environments_and_switches_between_them() {
        let c = conn();
        let st = status(&c).unwrap();
        assert_eq!(st.environment, Environment::Sandbox);
        assert!(!st.configured);
        assert!(st.environments.iter().all(|e| !e.configured));
        assert!(require(&c).unwrap_err().to_string().contains("sandbox"));

        save_env(&c, "sandbox", ID_A);
        save_env(&c, "production", ID_B);
        let st = status(&c).unwrap();
        assert_eq!(st.environment, Environment::Production, "saving selects that environment");
        assert_eq!(st.client_id_hint.as_deref(), Some("cdef"));
        assert!(st.environments.iter().all(|e| e.configured));

        // Switching does not need the keys again.
        let st = set_active_environment(&c, Environment::Sandbox).unwrap();
        assert_eq!(st.environment, Environment::Sandbox);
        assert!(st.configured);
        assert_eq!(load(&c).unwrap().unwrap().client_id, ID_A);
        assert_eq!(require(&c).unwrap().environment, Environment::Sandbox);
        set_active_environment(&c, Environment::Production).unwrap();
        assert_eq!(load(&c).unwrap().unwrap().client_id, ID_B);

        // Switching to an environment with no keys is allowed but not usable.
        clear(&c, Some(Environment::Production)).unwrap();
        let st = status(&c).unwrap();
        assert!(!st.configured);
        assert!(st.environments[0].configured, "sandbox keys are untouched");
        assert!(require(&c).unwrap_err().to_string().contains("production"));

        clear(&c, None).unwrap();
        assert!(status(&c).unwrap().environments.iter().all(|e| !e.configured));
    }

    #[test]
    fn debug_output_redacts_secrets() {
        let c = validate("sandbox", ID_A, &"f".repeat(30)).unwrap();
        let text = format!("{c:?}");
        assert!(!text.contains("ffff"));
        assert!(!text.contains(ID_A));
        assert!(text.contains("…4567"));
    }

    #[test]
    fn secrets_never_serialize() {
        let c = conn();
        save_env(&c, "sandbox", ID_A);
        let json = serde_json::to_string(&status(&c).unwrap()).unwrap();
        assert!(!json.contains("ffff"));
        assert!(!json.contains(ID_A), "only the last 4 characters are shown");
    }

    #[test]
    fn migrates_keys_from_the_single_environment_layout() {
        let c = conn();
        settings::set(&c, "plaid.environment", "production").unwrap();
        settings::set(&c, "plaid.client_id", ID_B).unwrap();
        settings::set(&c, "plaid.secret", &"f".repeat(30)).unwrap();

        let st = status(&c).unwrap();
        assert_eq!(st.environment, Environment::Production);
        assert!(st.configured);
        assert_eq!(load(&c).unwrap().unwrap().client_id, ID_B);
        assert_eq!(settings::get(&c, "plaid.client_id").unwrap(), None, "legacy keys are cleaned up");
        assert_eq!(settings::get(&c, "plaid.secret").unwrap(), None);
        // Sandbox is still empty, and switching there doesn't lose production.
        set_active_environment(&c, Environment::Sandbox).unwrap();
        assert!(!status(&c).unwrap().configured);
        set_active_environment(&c, Environment::Production).unwrap();
        assert_eq!(load(&c).unwrap().unwrap().client_id, ID_B);
    }

    #[test]
    fn validation() {
        assert!(validate("development", ID_A, &"f".repeat(30)).is_err());
        assert!(validate("sandbox", "short", &"f".repeat(30)).is_err());
        assert!(validate("sandbox", ID_A, "has spaces in it!!!!!").is_err());
        assert_eq!(validate("sandbox", &format!("  {ID_A}  "), &"f".repeat(30)).unwrap().client_id, ID_A);
    }

    #[test]
    fn user_id_is_stable() {
        let c = conn();
        let a = client_user_id(&c).unwrap();
        assert!(a.starts_with("local-"));
        assert_eq!(a, client_user_id(&c).unwrap());
    }
}
