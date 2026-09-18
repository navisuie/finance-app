mod bank;
mod calculators;
mod commands;
mod data;
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
            data::list_accounts,
            data::test_data_summary,
            data::delete_test_data,
            data::get_account,
            data::create_account,
            data::update_account,
            data::set_account_archived,
            data::delete_account,
            data::set_balance,
            data::delete_balance,
            data::balance_history,
            data::net_worth,
            data::debt_summary,
            data::net_worth_history,
            data::budget_month,
            data::set_budget,
            data::copy_budgets,
            data::list_goals,
            data::create_goal,
            data::update_goal,
            data::delete_goal,
            data::list_categories,
            data::create_category,
            data::update_category,
            data::delete_category,
            data::query_transactions,
            data::create_transaction,
            data::update_transaction,
            data::delete_transactions,
            data::set_transactions_category,
            data::suggest_category,
            data::period_summary,
            data::list_transfer_candidates,
            data::confirm_transfer_link,
            data::dismiss_transfer_candidate,
            data::unlink_transfer,
            data::import_detect,
            data::import_preview,
            data::import_commit,
            data::list_import_profiles,
            data::save_import_profile,
            data::delete_import_profile,
            bank::plaid_status,
            bank::plaid_save_credentials,
            bank::plaid_clear_credentials,
            bank::plaid_set_environment,
            bank::plaid_list_items,
            bank::plaid_start_link,
            bank::plaid_start_reconnect,
            bank::plaid_reopen_link,
            bank::plaid_poll_link,
            bank::plaid_cancel_link,
            bank::plaid_item_accounts,
            bank::plaid_link_accounts,
            bank::plaid_sync_item,
            bank::plaid_sync_all,
            bank::plaid_remove_item,
            bank::plaid_sandbox_add_item,
            bank::list_holdings,
            calculators::calc_retirement,
            calculators::calc_retirement_monte_carlo,
            calculators::calc_savings_goal,
            calculators::calc_home_affordability,
            calculators::calc_rent_vs_buy,
            calculators::calc_debt_payoff,
            calculators::list_scenarios,
            calculators::save_scenario,
            calculators::delete_scenario,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the finance app");
}
