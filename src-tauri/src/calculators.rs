//! IPC commands for the six calculators and saved scenarios.
//!
//! The calculators themselves are pure functions with no database access, so
//! these commands don't touch `AppState` at all — no session, no lock check.
//! Only scenario save/list/delete need the vault.

use finance_core::calculators::debt_payoff::{self, DebtPayoffInput, DebtPayoffResult};
use finance_core::calculators::home::{self, HomeAffordabilityInput, HomeAffordabilityResult};
use finance_core::calculators::rent_vs_buy::{self, RentVsBuyInput, RentVsBuyResult};
use finance_core::calculators::retirement::{self, MonteCarloInput, MonteCarloResult, RetirementInput, RetirementResult};
use finance_core::calculators::savings_goal::{self, SavingsGoalInput, SavingsGoalResult};
use finance_core::scenarios::{self, Scenario};
use serde_json::Value;
use tauri::State;

use crate::commands::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn calc_retirement(input: RetirementInput) -> CmdResult<RetirementResult> {
    Ok(retirement::project(&input)?)
}

#[tauri::command]
pub fn calc_retirement_monte_carlo(input: MonteCarloInput) -> CmdResult<MonteCarloResult> {
    Ok(retirement::monte_carlo(&input)?)
}

#[tauri::command]
pub fn calc_savings_goal(input: SavingsGoalInput) -> CmdResult<SavingsGoalResult> {
    Ok(savings_goal::project(&input)?)
}

#[tauri::command]
pub fn calc_home_affordability(input: HomeAffordabilityInput) -> CmdResult<HomeAffordabilityResult> {
    Ok(home::project(&input)?)
}

#[tauri::command]
pub fn calc_rent_vs_buy(input: RentVsBuyInput) -> CmdResult<RentVsBuyResult> {
    Ok(rent_vs_buy::project(&input)?)
}

#[tauri::command]
pub fn calc_debt_payoff(input: DebtPayoffInput) -> CmdResult<DebtPayoffResult> {
    Ok(debt_payoff::project(&input)?)
}

#[tauri::command]
pub fn list_scenarios(state: State<'_, AppState>, calculator: String) -> CmdResult<Vec<Scenario>> {
    Ok(state.with_session(|s| scenarios::list(s.conn(), &calculator))?)
}

#[tauri::command]
pub fn save_scenario(state: State<'_, AppState>, calculator: String, name: String, inputs: Value) -> CmdResult<Scenario> {
    Ok(state.with_session(|s| scenarios::create(s.conn(), &calculator, &name, &inputs))?)
}

#[tauri::command]
pub fn delete_scenario(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(state.with_session(|s| scenarios::delete(s.conn(), id))?)
}
