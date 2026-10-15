//! IPC commands for accounts, categories, transactions and CSV import.
//! Every command runs through `AppState::with_session`, so all of them fail
//! with `locked` when the vault is locked or the idle timer has expired.

use std::collections::HashMap;

use finance_core::accounts::{
    self, Account, AccountInput, BalanceSnapshot, NetWorth, NetWorthPoint, Scope, TestDataSummary,
};
use finance_core::budgets::{self, BudgetDraft, BudgetMonth, IncomeBaseline, LimitInput, SavingMonth};
use finance_core::goals::{self, Goal, GoalInput};
use finance_core::plaid::api::Environment;
use finance_core::plaid::config;
use finance_core::categories::{self, Category, CategoryInput};
use finance_core::debt::{self, DebtSummary};
use finance_core::import::{self, CsvMapping, Detection, ImportProfile, ImportResult, Preview};
use finance_core::transactions::{
    self, PeriodSummary, Transaction, TransactionFilter, TransactionInput, TransactionPage,
};
use finance_core::transfers::{self, TransferCandidate};
use tauri::{AppHandle, State};

use crate::commands::{blocking, CmdResult};
use crate::state::AppState;

/// What the app shows right now: Sandbox is a self-contained test world, so
/// while it is active every list and total covers test data only; otherwise
/// they cover real data only.
fn active_scope(state: &AppState) -> finance_core::Result<Scope> {
    let env = state.with_session_passive(|s| config::active_environment(s.conn()))?;
    Ok(match env {
        Environment::Sandbox => Scope::Test,
        Environment::Production => Scope::Real,
    })
}

// ---------------------------------------------------------------- accounts

#[tauri::command]
pub fn list_accounts(
    state: State<'_, AppState>,
    include_archived: bool,
    scope: Option<Scope>,
) -> CmdResult<Vec<Account>> {
    let scope = match scope {
        Some(s) => s,
        None => active_scope(&state)?,
    };
    Ok(state.with_session(|s| accounts::list(s.conn(), include_archived, scope))?)
}

/// How much Sandbox test data is in the vault.
#[tauri::command]
pub fn test_data_summary(state: State<'_, AppState>) -> CmdResult<TestDataSummary> {
    Ok(state.with_session(|s| accounts::test_data_summary(s.conn()))?)
}

/// Delete every account created by a Sandbox connection, with its
/// transactions and balances. Real accounts are never touched.
#[tauri::command]
pub fn delete_test_data(state: State<'_, AppState>) -> CmdResult<TestDataSummary> {
    Ok(state.with_session(|s| accounts::delete_test_data(s.conn_mut()))?)
}

#[tauri::command]
pub fn get_account(state: State<'_, AppState>, id: i64) -> CmdResult<Account> {
    Ok(state.with_session(|s| accounts::get(s.conn(), id))?)
}

#[tauri::command]
pub fn create_account(state: State<'_, AppState>, input: AccountInput) -> CmdResult<Account> {
    Ok(state.with_session(|s| accounts::create(s.conn_mut(), &input))?)
}

#[tauri::command]
pub fn update_account(state: State<'_, AppState>, id: i64, input: AccountInput) -> CmdResult<Account> {
    Ok(state.with_session(|s| accounts::update(s.conn(), id, &input))?)
}

#[tauri::command]
pub fn set_account_archived(state: State<'_, AppState>, id: i64, archived: bool) -> CmdResult<Account> {
    Ok(state.with_session(|s| accounts::set_archived(s.conn(), id, archived))?)
}

#[tauri::command]
pub fn delete_account(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| accounts::delete(s.conn(), id))?)
}

#[tauri::command]
pub fn set_balance(
    state: State<'_, AppState>,
    account_id: i64,
    as_of: String,
    amount_cents: i64,
) -> CmdResult<Account> {
    Ok(state.with_session(|s| {
        accounts::set_balance(s.conn(), account_id, &as_of, amount_cents)?;
        // A statement balance re-anchors a debt's running total.
        debt::refresh_auto_balance(s.conn(), account_id)?;
        accounts::get(s.conn(), account_id)
    })?)
}

#[tauri::command]
pub fn delete_balance(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| accounts::delete_balance(s.conn(), id))?)
}

#[tauri::command]
pub fn balance_history(state: State<'_, AppState>, account_id: i64) -> CmdResult<Vec<BalanceSnapshot>> {
    Ok(state.with_session(|s| accounts::balance_history(s.conn(), account_id))?)
}

/// Payoff projection and linked payments for a debt account.
#[tauri::command]
pub fn debt_summary(state: State<'_, AppState>, account_id: i64) -> CmdResult<DebtSummary> {
    Ok(state.with_session(|s| debt::summary(s.conn(), account_id))?)
}

#[tauri::command]
pub fn net_worth(state: State<'_, AppState>) -> CmdResult<NetWorth> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| accounts::net_worth(s.conn(), scope))?)
}

/// Net worth at the end of each of the last `months` months.
#[tauri::command]
pub fn net_worth_history(state: State<'_, AppState>, months: u32) -> CmdResult<Vec<NetWorthPoint>> {
    let scope = active_scope(&state)?;
    let months = months.clamp(3, 60);
    Ok(state.with_session(|s| accounts::net_worth_history(s.conn(), scope, months))?)
}

// ----------------------------------------------------------------- budgets

#[tauri::command]
pub fn budget_month(state: State<'_, AppState>, month: String) -> CmdResult<BudgetMonth> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| budgets::month_view(s.conn(), &month, scope))?)
}

/// Set a category's limit for a month; 0 removes it.
#[tauri::command]
pub fn set_budget(
    state: State<'_, AppState>,
    category_id: i64,
    month: String,
    limit_cents: i64,
) -> CmdResult<BudgetMonth> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| {
        budgets::set_limit(s.conn(), category_id, &month, limit_cents)?;
        budgets::month_view(s.conn(), &month, scope)
    })?)
}

/// Average monthly income before `month`, and what this month's savings
/// rate should be measured against.
#[tauri::command]
pub fn income_baseline(state: State<'_, AppState>, month: String) -> CmdResult<IncomeBaseline> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| budgets::income_baseline(s.conn(), &month, scope))?)
}

/// A suggested budget for `month`, derived from the months before it.
#[tauri::command]
pub fn budget_draft(state: State<'_, AppState>, month: String) -> CmdResult<BudgetDraft> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| budgets::draft(s.conn(), &month, scope))?)
}

#[tauri::command]
pub fn apply_budget_plan(
    state: State<'_, AppState>,
    month: String,
    limits: Vec<LimitInput>,
) -> CmdResult<BudgetMonth> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| {
        budgets::set_limits(s.conn_mut(), &month, &limits)?;
        budgets::month_view(s.conn(), &month, scope)
    })?)
}

/// What was moved into savings and investment accounts in `month`.
#[tauri::command]
pub fn saving_month(state: State<'_, AppState>, month: String) -> CmdResult<SavingMonth> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| budgets::saving_month(s.conn(), &month, scope))?)
}

/// Set the standing monthly saving & investing target (0 removes it), and
/// return `month` with it applied.
#[tauri::command]
pub fn set_saving_target(state: State<'_, AppState>, month: String, cents: i64) -> CmdResult<SavingMonth> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| {
        budgets::set_saving_target(s.conn(), cents)?;
        budgets::saving_month(s.conn(), &month, scope)
    })?)
}

#[tauri::command]
pub fn copy_budgets(state: State<'_, AppState>, from: String, to: String) -> CmdResult<BudgetMonth> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| {
        budgets::copy_month(s.conn_mut(), &from, &to)?;
        budgets::month_view(s.conn(), &to, scope)
    })?)
}

// ------------------------------------------------------------------- goals

#[tauri::command]
pub fn list_goals(state: State<'_, AppState>) -> CmdResult<Vec<Goal>> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| goals::list(s.conn(), scope))?)
}

#[tauri::command]
pub fn create_goal(state: State<'_, AppState>, input: GoalInput) -> CmdResult<Goal> {
    Ok(state.with_session(|s| goals::create(s.conn(), &input))?)
}

#[tauri::command]
pub fn update_goal(state: State<'_, AppState>, id: i64, input: GoalInput) -> CmdResult<Goal> {
    Ok(state.with_session(|s| goals::update(s.conn(), id, &input))?)
}

#[tauri::command]
pub fn delete_goal(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| goals::delete(s.conn(), id))?)
}

// -------------------------------------------------------------- categories

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> CmdResult<Vec<Category>> {
    Ok(state.with_session(|s| categories::list(s.conn()))?)
}

#[tauri::command]
pub fn create_category(state: State<'_, AppState>, input: CategoryInput) -> CmdResult<Category> {
    Ok(state.with_session(|s| categories::create(s.conn(), &input))?)
}

#[tauri::command]
pub fn update_category(state: State<'_, AppState>, id: i64, input: CategoryInput) -> CmdResult<()> {
    Ok(state.with_session(|s| categories::update(s.conn(), id, &input))?)
}

#[tauri::command]
pub fn delete_category(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| categories::delete(s.conn(), id))?)
}

// ------------------------------------------------------------ transactions

#[tauri::command]
pub fn query_transactions(
    state: State<'_, AppState>,
    mut filter: TransactionFilter,
) -> CmdResult<TransactionPage> {
    if filter.scope.is_none() {
        filter.scope = Some(active_scope(&state)?);
    }
    Ok(state.with_session(|s| transactions::query(s.conn(), &filter))?)
}

#[tauri::command]
pub fn create_transaction(state: State<'_, AppState>, input: TransactionInput) -> CmdResult<Transaction> {
    Ok(state.with_session(|s| {
        let t = transactions::create(s.conn(), &input)?;
        // A payment tagged to a debt moves that debt's balance.
        debt::refresh_all_auto_balances(s.conn())?;
        Ok(t)
    })?)
}

#[tauri::command]
pub fn update_transaction(
    state: State<'_, AppState>,
    id: i64,
    input: TransactionInput,
) -> CmdResult<Transaction> {
    Ok(state.with_session(|s| {
        let t = transactions::update(s.conn(), id, &input)?;
        debt::refresh_all_auto_balances(s.conn())?;
        Ok(t)
    })?)
}

#[tauri::command]
pub fn delete_transactions(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<usize> {
    Ok(state.with_session(|s| {
        let n = transactions::delete_many(s.conn_mut(), &ids)?;
        // Removing a payment puts that much back on the debt.
        debt::refresh_all_auto_balances(s.conn())?;
        Ok(n)
    })?)
}

#[tauri::command]
pub fn set_transactions_category(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    category_id: Option<i64>,
) -> CmdResult<usize> {
    Ok(state.with_session(|s| transactions::set_category(s.conn_mut(), &ids, category_id))?)
}

#[tauri::command]
pub fn suggest_category(state: State<'_, AppState>, payee: String) -> CmdResult<Option<i64>> {
    Ok(state.with_session(|s| transactions::suggest_category(s.conn(), &payee))?)
}

/// The account a recurring payment to this payee was last tagged as paying,
/// so the form can offer it again.
#[tauri::command]
pub fn suggest_transfer_account(
    state: State<'_, AppState>,
    account_id: i64,
    payee: String,
) -> CmdResult<Option<i64>> {
    Ok(state.with_session(|s| transactions::suggest_transfer_account(s.conn(), account_id, &payee))?)
}

#[tauri::command]
pub fn period_summary(state: State<'_, AppState>, from: String, to: String) -> CmdResult<PeriodSummary> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| transactions::summary(s.conn(), &from, &to, scope))?)
}

// -------------------------------------------------------------- transfers

#[tauri::command]
pub fn list_transfer_candidates(state: State<'_, AppState>) -> CmdResult<Vec<TransferCandidate>> {
    let scope = active_scope(&state)?;
    Ok(state.with_session(|s| transfers::find_candidates(s.conn(), scope))?)
}

#[tauri::command]
pub fn confirm_transfer_link(state: State<'_, AppState>, a_id: i64, b_id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| transfers::confirm_link(s.conn_mut(), a_id, b_id))?)
}

#[tauri::command]
pub fn dismiss_transfer_candidate(state: State<'_, AppState>, a_id: i64, b_id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| transfers::dismiss_candidate(s.conn(), a_id, b_id))?)
}

#[tauri::command]
pub fn unlink_transfer(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| transfers::unlink(s.conn_mut(), id))?)
}

// ------------------------------------------------------------------ import
// Parsing can take a moment for large files, so these run off the main thread.

#[tauri::command]
pub async fn import_detect(app: AppHandle, account_id: i64, text: String) -> CmdResult<Detection> {
    blocking(app, move |state| state.with_session(|s| import::detect(s.conn(), account_id, &text))).await
}

#[tauri::command]
pub async fn import_preview(
    app: AppHandle,
    account_id: i64,
    text: String,
    mapping: CsvMapping,
) -> CmdResult<Preview> {
    blocking(app, move |state| {
        state.with_session(|s| import::preview(s.conn(), account_id, &text, &mapping))
    })
    .await
}

#[tauri::command]
pub async fn import_commit(
    app: AppHandle,
    account_id: i64,
    text: String,
    mapping: CsvMapping,
    lines: Vec<usize>,
    category_overrides: HashMap<usize, Option<i64>>,
) -> CmdResult<ImportResult> {
    blocking(app, move |state| {
        state.with_session(|s| {
            let r =
                import::commit(s.conn_mut(), account_id, &text, &mapping, &lines, &category_overrides)?;
            // Imported payments tagged to a debt move its balance.
            debt::refresh_all_auto_balances(s.conn())?;
            Ok(r)
        })
    })
    .await
}

#[tauri::command]
pub fn list_import_profiles(state: State<'_, AppState>) -> CmdResult<Vec<ImportProfile>> {
    Ok(state.with_session(|s| import::list_profiles(s.conn()))?)
}

#[tauri::command]
pub fn save_import_profile(
    state: State<'_, AppState>,
    name: String,
    mapping: CsvMapping,
) -> CmdResult<ImportProfile> {
    Ok(state.with_session(|s| import::save_profile(s.conn(), &name, &mapping))?)
}

#[tauri::command]
pub fn delete_import_profile(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| import::delete_profile(s.conn(), id))?)
}
