//! IPC commands callable from the UI. Every command that touches data goes
//! through `AppState::with_session`, which refuses when locked.

use finance_core::error::ErrorPayload;
use finance_core::settings::{self, AppSettings};
use finance_core::{CoreError, Vault};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

use crate::state::AppState;

pub const EVT_LOCKED: &str = "vault-locked";

#[derive(Debug, Serialize)]
pub struct CmdError(ErrorPayload);

impl From<CoreError> for CmdError {
    fn from(e: CoreError) -> Self {
        CmdError(ErrorPayload::from(&e))
    }
}

type CmdResult<T> = std::result::Result<T, CmdError>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    exists: bool,
    unlocked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyOut {
    recovery_key: String,
}

/// Run CPU-heavy key derivation off the main thread so the window stays
/// responsive.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> finance_core::Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        f(&state)
    })
    .await
    .map_err(|_| CmdError::from(CoreError::Invalid("Background task failed.".into())))?
    .map_err(CmdError::from)
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> VaultStatus {
    state.enforce_idle_timeout();
    VaultStatus { exists: Vault::exists(&state.data_dir), unlocked: state.is_unlocked() }
}

#[tauri::command]
pub async fn vault_create(app: AppHandle, password: String) -> CmdResult<RecoveryKeyOut> {
    let password = Zeroizing::new(password);
    blocking(app, move |state| {
        let opened = Vault::create(&state.data_dir, &password)?;
        let minutes = settings::load(opened.session.conn())?.auto_lock_minutes;
        state.set_session(opened.session, minutes);
        Ok(RecoveryKeyOut { recovery_key: opened.recovery_key.to_string() })
    })
    .await
}

#[tauri::command]
pub async fn vault_unlock(app: AppHandle, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    blocking(app, move |state| {
        state.throttle.lock().map_err(|_| CoreError::Crypto)?.check()?;
        match Vault::unlock(&state.data_dir, &password) {
            Ok(session) => {
                state.throttle.lock().map_err(|_| CoreError::Crypto)?.record_success();
                let minutes = settings::load(session.conn())?.auto_lock_minutes;
                state.set_session(session, minutes);
                Ok(())
            }
            Err(e) => {
                if matches!(e, CoreError::WrongPassword) {
                    state.throttle.lock().map_err(|_| CoreError::Crypto)?.record_failure();
                }
                Err(e)
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn vault_recover(
    app: AppHandle,
    recovery_key: String,
    new_password: String,
) -> CmdResult<RecoveryKeyOut> {
    let recovery_key = Zeroizing::new(recovery_key);
    let new_password = Zeroizing::new(new_password);
    blocking(app, move |state| {
        state.throttle.lock().map_err(|_| CoreError::Crypto)?.check()?;
        match Vault::recover(&state.data_dir, &recovery_key, &new_password) {
            Ok(opened) => {
                state.throttle.lock().map_err(|_| CoreError::Crypto)?.record_success();
                let minutes = settings::load(opened.session.conn())?.auto_lock_minutes;
                state.set_session(opened.session, minutes);
                Ok(RecoveryKeyOut { recovery_key: opened.recovery_key.to_string() })
            }
            Err(e) => {
                if matches!(e, CoreError::InvalidRecoveryKey) {
                    state.throttle.lock().map_err(|_| CoreError::Crypto)?.record_failure();
                }
                Err(e)
            }
        }
    })
    .await
}

#[tauri::command]
pub fn vault_lock(app: AppHandle, state: State<'_, AppState>) {
    if state.lock() {
        let _ = app.emit(EVT_LOCKED, "manual");
    }
}

/// Heartbeat from the UI when the user is active.
#[tauri::command]
pub fn activity(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.touch()?)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    Ok(state.with_session(|s| settings::load(s.conn()))?)
}

#[tauri::command]
pub fn set_auto_lock(state: State<'_, AppState>, minutes: u32) -> CmdResult<AppSettings> {
    let out = state.with_session(|s| {
        settings::set_auto_lock_minutes(s.conn(), minutes)?;
        settings::load(s.conn())
    })?;
    state.set_auto_lock(out.auto_lock_minutes);
    Ok(out)
}

#[tauri::command]
pub async fn change_password(
    app: AppHandle,
    current_password: String,
    new_password: String,
) -> CmdResult<()> {
    let current = Zeroizing::new(current_password);
    let new = Zeroizing::new(new_password);
    blocking(app, move |state| {
        state.throttle.lock().map_err(|_| CoreError::Crypto)?.check()?;
        let r = state.with_session(|s| s.change_password(&current, &new));
        if matches!(r, Err(CoreError::WrongPassword)) {
            state.throttle.lock().map_err(|_| CoreError::Crypto)?.record_failure();
        }
        r
    })
    .await
}
