mod commands;
mod state;

use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::commands::EVT_LOCKED;
use crate::state::AppState;

/// How often the backend checks the idle timer.
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(10);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // ~/Library/Application Support/com.navi.financeapp on macOS
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState::new(data_dir));

            // Backend-enforced auto-lock: independent of the UI, so it still
            // fires if the webview is frozen or tampered with.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(IDLE_CHECK_INTERVAL);
                let state = handle.state::<AppState>();
                if state.enforce_idle_timeout() {
                    let _ = handle.emit(EVT_LOCKED, "idle");
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Lock as soon as the window is closed or destroyed.
            if matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            ) {
                window.app_handle().state::<AppState>().lock();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::vault_create,
            commands::vault_unlock,
            commands::vault_recover,
            commands::vault_lock,
            commands::activity,
            commands::get_settings,
            commands::set_auto_lock,
            commands::change_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the finance app");
}
