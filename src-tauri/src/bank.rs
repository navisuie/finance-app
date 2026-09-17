//! IPC commands for Plaid bank connections.
//!
//! Network calls never run while the vault mutex is held: every flow in
//! `finance_core::plaid::service` reads from the vault, releases it, talks to
//! Plaid, then writes. Vault access here is "passive" (doesn't reset the
//! auto-lock timer), except that an explicit user action counts as activity.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use finance_core::plaid::api::{validate_link_url, Environment, PlaidApi};
use finance_core::plaid::config::{self, PlaidStatus};
use finance_core::plaid::service::{self, LinkMode, LinkPoll, LinkStart, SyncResult};
use finance_core::plaid::store::{self, AccountChoice, HoldingView, ItemView, LinkSummary, PlaidAccountView};
use finance_core::plaid::HttpPlaid;
use finance_core::{CoreError, Result};
use serde::Serialize;
use tauri::{AppHandle, State};
use zeroize::Zeroizing;

use crate::commands::{blocking, CmdError, CmdResult};
use crate::state::{AppState, PendingLink};

/// Link sessions older than this are abandoned (Hosted Link URLs last 30 min).
const LINK_TIMEOUT: Duration = Duration::from_secs(35 * 60);

fn client(state: &AppState) -> Result<HttpPlaid> {
    state.with_session_passive(|s| Ok(config::require(s.conn())?.client()))
}

fn open_in_browser(url: &str) -> Result<()> {
    let url = validate_link_url(url)?;
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("/usr/bin/open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = Command::new("xdg-open");
    #[cfg(not(unix))]
    return Err(CoreError::Invalid(format!("Open this link in your browser: {url}")));
    // No shell is involved; the URL is passed as a single argument.
    #[cfg(unix)]
    {
        cmd.arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CoreError::Invalid(format!("Couldn't open your browser: {e}")))?;
        Ok(())
    }
}

#[tauri::command]
pub fn plaid_status(state: State<'_, AppState>) -> CmdResult<PlaidStatus> {
    Ok(state.with_session(|s| config::status(s.conn()))?)
}

/// Verify the keys with Plaid, then store them in the vault.
#[tauri::command]
pub async fn plaid_save_credentials(
    app: AppHandle,
    environment: String,
    client_id: String,
    secret: String,
) -> CmdResult<PlaidStatus> {
    let secret = Zeroizing::new(secret);
    blocking(app, move |state| {
        state.touch()?;
        let creds = config::validate(&environment, &client_id, &secret)?;
        creds.client().check_credentials()?;
        state.with_session_passive(|s| {
            config::save(s.conn(), &creds)?;
            config::status(s.conn())
        })
    })
    .await
}

/// Remove the keys for one environment ("sandbox" / "production"), or for
/// both when no environment is given.
#[tauri::command]
pub fn plaid_clear_credentials(
    state: State<'_, AppState>,
    environment: Option<String>,
) -> CmdResult<PlaidStatus> {
    let env = match environment.as_deref() {
        Some(e) => Some(config::parse_environment(e).map_err(CmdError::from)?),
        None => None,
    };
    Ok(state.with_session(|s| {
        config::clear(s.conn(), env)?;
        config::status(s.conn())
    })?)
}

/// Switch between Sandbox and Production. Keys for both are kept, so no
/// network call is needed. Entering Production asks for the master password
/// (once every 10 minutes): live bank data shouldn't be one stray click away.
#[tauri::command]
pub async fn plaid_set_environment(
    app: AppHandle,
    environment: String,
    password: Option<String>,
) -> CmdResult<PlaidStatus> {
    let password = password.map(Zeroizing::new);
    blocking(app, move |state| {
        state.touch()?;
        let env = config::parse_environment(&environment)?;
        if env == Environment::Production && !state.production_confirmed() {
            match password {
                Some(pw) => state.confirm_production(&pw)?,
                None => return Err(CoreError::PasswordRequired),
            }
        }
        // Any half-finished Link session belongs to the old environment.
        if let Ok(mut pending) = state.pending_links.lock() {
            pending.clear();
        }
        state.with_session_passive(|s| config::set_active_environment(s.conn(), env))
    })
    .await
}

/// Production actions (connecting or reconnecting a bank) need a recent
/// password confirmation too.
fn guard_production(state: &AppState) -> Result<()> {
    let env = state.with_session_passive(|s| config::active_environment(s.conn()))?;
    if env == Environment::Production {
        state.require_production_confirmed()?;
    }
    Ok(())
}

#[tauri::command]
pub fn plaid_list_items(state: State<'_, AppState>) -> CmdResult<Vec<ItemView>> {
    Ok(state.with_session(|s| store::list_items(s.conn()))?)
}

fn begin(state: &AppState, mode: LinkMode) -> Result<LinkStart> {
    state.touch()?;
    guard_production(state)?;
    let api = client(state)?;
    let start = service::start_link(&api, state, &mode)?;
    {
        let mut pending = state.pending_links.lock().map_err(|_| CoreError::Crypto)?;
        pending.retain(|_, p| p.started.elapsed() < LINK_TIMEOUT);
        pending.insert(
            start.link_token.clone(),
            PendingLink { mode, url: start.url.clone(), started: Instant::now() },
        );
    }
    open_in_browser(&start.url)?;
    Ok(start)
}

/// Start connecting a new institution. `kind` is "banking" or "investments".
#[tauri::command]
pub async fn plaid_start_link(app: AppHandle, kind: String) -> CmdResult<LinkStart> {
    blocking(app, move |state| {
        let products = service::products_for(&kind)?;
        begin(state, LinkMode::New { products })
    })
    .await
}

/// Re-authenticate an existing connection (update mode).
#[tauri::command]
pub async fn plaid_start_reconnect(app: AppHandle, item_id: i64) -> CmdResult<LinkStart> {
    blocking(app, move |state| begin(state, LinkMode::Update { item_id })).await
}

/// Re-open the browser for a link that's in progress.
#[tauri::command]
pub fn plaid_reopen_link(state: State<'_, AppState>, link_token: String) -> CmdResult<()> {
    state.touch()?;
    let url = state
        .pending_links
        .lock()
        .map_err(|_| CoreError::Crypto)?
        .get(&link_token)
        .map(|p| p.url.clone())
        .ok_or_else(|| CoreError::Invalid("That connection attempt has ended. Start again.".into()))?;
    Ok(open_in_browser(&url)?)
}

#[tauri::command]
pub async fn plaid_poll_link(app: AppHandle, link_token: String) -> CmdResult<LinkPoll> {
    blocking(app, move |state| {
        let mode = {
            let pending = state.pending_links.lock().map_err(|_| CoreError::Crypto)?;
            let p = pending
                .get(&link_token)
                .ok_or_else(|| CoreError::Invalid("That connection attempt has ended. Start again.".into()))?;
            if p.started.elapsed() > LINK_TIMEOUT {
                None
            } else {
                Some(p.mode.clone())
            }
        };
        let Some(mode) = mode else {
            state.pending_links.lock().map_err(|_| CoreError::Crypto)?.remove(&link_token);
            return Ok(LinkPoll::Exited { message: "The connection link expired. Start again.".into() });
        };
        // The user is actively connecting in their browser; keep the vault
        // open for the (time-limited) duration of the link.
        state.touch()?;
        let api = client(state)?;
        let result = service::poll_link(&api, state, &link_token, &mode)?;
        if !matches!(result, LinkPoll::Waiting) {
            state.pending_links.lock().map_err(|_| CoreError::Crypto)?.remove(&link_token);
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
pub fn plaid_cancel_link(state: State<'_, AppState>, link_token: String) -> CmdResult<()> {
    state.pending_links.lock().map_err(|_| CoreError::Crypto)?.remove(&link_token);
    Ok(())
}

#[tauri::command]
pub async fn plaid_item_accounts(app: AppHandle, item_id: i64) -> CmdResult<Vec<PlaidAccountView>> {
    blocking(app, move |state| {
        state.touch()?;
        let api = client(state)?;
        service::item_accounts(&api, state, item_id)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkAccountsOut {
    summary: LinkSummary,
    sync: SyncResult,
}

#[tauri::command]
pub async fn plaid_link_accounts(
    app: AppHandle,
    item_id: i64,
    choices: Vec<AccountChoice>,
) -> CmdResult<LinkAccountsOut> {
    blocking(app, move |state| {
        state.touch()?;
        guard_production(state)?;
        let api = client(state)?;
        let (summary, sync) = service::link_accounts(&api, state, item_id, &choices)?;
        Ok(LinkAccountsOut { summary, sync })
    })
    .await
}

#[tauri::command]
pub async fn plaid_sync_item(app: AppHandle, item_id: i64) -> CmdResult<SyncResult> {
    blocking(app, move |state| {
        state.touch()?;
        let api = client(state)?;
        service::sync_item(&api, state, item_id)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    item_id: i64,
    result: Option<SyncResult>,
    error: Option<String>,
}

/// Sync every connection. Does not count as user activity, so it can run in
/// the background after unlocking.
#[tauri::command]
pub async fn plaid_sync_all(app: AppHandle) -> CmdResult<Vec<SyncOutcome>> {
    blocking(app, move |state| {
        let api = client(state)?;
        Ok(service::sync_all(&api, state)?
            .into_iter()
            .map(|r| match r {
                Ok(res) => SyncOutcome { item_id: res.item_id, result: Some(res), error: None },
                Err((item_id, error)) => SyncOutcome { item_id, result: None, error: Some(error) },
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn plaid_remove_item(app: AppHandle, item_id: i64, force: bool) -> CmdResult<()> {
    blocking(app, move |state| {
        state.touch()?;
        let api = client(state)?;
        service::remove_item(&api, state, item_id, force)
    })
    .await
}

/// Sandbox only: add a test bank without the browser.
#[tauri::command]
pub async fn plaid_sandbox_add_item(app: AppHandle, kind: String) -> CmdResult<LinkPoll> {
    blocking(app, move |state| {
        state.touch()?;
        let products = service::products_for(&kind)?;
        let api = client(state)?;
        service::add_sandbox_item(&api, state, &products)
    })
    .await
}

#[tauri::command]
pub fn list_holdings(state: State<'_, AppState>, account_id: i64) -> CmdResult<Vec<HoldingView>> {
    Ok(state.with_session(|s| store::list_holdings(s.conn(), account_id))?)
}
