import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Account,
  AccountInput,
  BalanceSnapshot,
  Category,
  BudgetMonth,
  BudgetDraft,
  IncomeBaseline,
  LimitInput,
  DebtSummary,
  Goal,
  GoalInput,
  NetWorthPoint,
  CategoryInput,
  CsvMapping,
  Detection,
  ImportPreview,
  ImportProfile,
  ImportResult,
  NetWorth,
  PeriodSummary,
  Transaction,
  TransactionFilter,
  TransactionInput,
  TransactionPage,
  AccountChoice,
  HoldingView,
  LinkPoll,
  LinkStart,
  PlaidAccountView,
  PlaidItem,
  PlaidStatus,
  SyncOutcome,
  SyncResult,
  Scope,
  TestDataSummary,
  TransferCandidate,
  RetirementInput,
  RetirementResult,
  MonteCarloInput,
  MonteCarloResult,
  SavingsGoalInput,
  SavingsGoalResult,
  HomeAffordabilityInput,
  HomeAffordabilityResult,
  RentVsBuyInput,
  RentVsBuyResult,
  DebtPayoffInput,
  DebtPayoffResult,
  BudgetPlanInput,
  BudgetPlanResult,
  Scenario,
} from "./types";

/** Error shape returned by every Rust command. */
export interface ApiError {
  code:
    | "vault_missing"
    | "vault_exists"
    | "locked"
    | "wrong_password"
    | "invalid_recovery_key"
    | "weak_password"
    | "too_many_attempts"
    | "corrupt_vault"
    | "invalid"
    | "crypto"
    | "db"
    | "io"
    | "json"
    | "plaid"
    | "password_required";
  message: string;
  retry_after_secs?: number;
  /** Plaid's error code when code === "plaid" */
  plaid_code?: string;
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export function errorMessage(e: unknown): string {
  if (isApiError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return "Something went wrong.";
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export interface AppSettings {
  autoLockMinutes: number;
}

export const api = {
  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  createVault: (password: string) =>
    invoke<{ recoveryKey: string }>("vault_create", { password }),
  unlock: (password: string) => invoke<void>("vault_unlock", { password }),
  recover: (recoveryKey: string, newPassword: string) =>
    invoke<{ recoveryKey: string }>("vault_recover", { recoveryKey, newPassword }),
  lock: () => invoke<void>("vault_lock"),
  activity: () => invoke<void>("activity"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  setAutoLock: (minutes: number) => invoke<AppSettings>("set_auto_lock", { minutes }),
  changePassword: (currentPassword: string, newPassword: string) =>
    invoke<void>("change_password", { currentPassword, newPassword }),
  /** Writes an encrypted backup to `path` (chosen via a native save dialog). */
  exportBackup: (password: string, path: string) => invoke<void>("export_backup", { password, path }),
  /** Restores a fresh vault from a backup file at `backupPath`. Only valid before a vault exists. */
  vaultRestore: (backupPath: string, password: string) => invoke<void>("vault_restore", { backupPath, password }),

  // Accounts
  /** `scope` defaults to the active environment's data. */
  listAccounts: (includeArchived = false, scope?: Scope) =>
    invoke<Account[]>("list_accounts", { includeArchived, scope: scope ?? null }),
  testDataSummary: () => invoke<TestDataSummary>("test_data_summary"),
  deleteTestData: () => invoke<TestDataSummary>("delete_test_data"),
  getAccount: (id: number) => invoke<Account>("get_account", { id }),
  createAccount: (input: AccountInput) => invoke<Account>("create_account", { input }),
  updateAccount: (id: number, input: AccountInput) => invoke<Account>("update_account", { id, input }),
  setAccountArchived: (id: number, archived: boolean) =>
    invoke<Account>("set_account_archived", { id, archived }),
  deleteAccount: (id: number) => invoke<void>("delete_account", { id }),
  setBalance: (accountId: number, asOf: string, amountCents: number) =>
    invoke<Account>("set_balance", { accountId, asOf, amountCents }),
  deleteBalance: (id: number) => invoke<void>("delete_balance", { id }),
  balanceHistory: (accountId: number) => invoke<BalanceSnapshot[]>("balance_history", { accountId }),
  netWorth: () => invoke<NetWorth>("net_worth"),
  debtSummary: (accountId: number) => invoke<DebtSummary>("debt_summary", { accountId }),
  netWorthHistory: (months = 13) => invoke<NetWorthPoint[]>("net_worth_history", { months }),

  // Budgets
  budgetMonth: (month: string) => invoke<BudgetMonth>("budget_month", { month }),
  setBudget: (categoryId: number, month: string, limitCents: number) =>
    invoke<BudgetMonth>("set_budget", { categoryId, month, limitCents }),
  copyBudgets: (from: string, to: string) => invoke<BudgetMonth>("copy_budgets", { from, to }),
  budgetDraft: (month: string) => invoke<BudgetDraft>("budget_draft", { month }),
  incomeBaseline: (month: string) => invoke<IncomeBaseline>("income_baseline", { month }),
  applyBudgetPlan: (month: string, limits: LimitInput[]) =>
    invoke<BudgetMonth>("apply_budget_plan", { month, limits }),

  // Goals
  listGoals: () => invoke<Goal[]>("list_goals"),
  createGoal: (input: GoalInput) => invoke<Goal>("create_goal", { input }),
  updateGoal: (id: number, input: GoalInput) => invoke<Goal>("update_goal", { id, input }),
  deleteGoal: (id: number) => invoke<void>("delete_goal", { id }),

  // Categories
  listCategories: () => invoke<Category[]>("list_categories"),
  createCategory: (input: CategoryInput) => invoke<Category>("create_category", { input }),
  updateCategory: (id: number, input: CategoryInput) => invoke<void>("update_category", { id, input }),
  deleteCategory: (id: number) => invoke<void>("delete_category", { id }),

  // Transactions
  queryTransactions: (filter: TransactionFilter) => invoke<TransactionPage>("query_transactions", { filter }),
  createTransaction: (input: TransactionInput) => invoke<Transaction>("create_transaction", { input }),
  updateTransaction: (id: number, input: TransactionInput) =>
    invoke<Transaction>("update_transaction", { id, input }),
  deleteTransactions: (ids: number[]) => invoke<number>("delete_transactions", { ids }),
  setTransactionsCategory: (ids: number[], categoryId: number | null) =>
    invoke<number>("set_transactions_category", { ids, categoryId }),
  suggestCategory: (payee: string) => invoke<number | null>("suggest_category", { payee }),
  /** The account a recurring payment to this payee was last tagged as paying. */
  suggestTransferAccount: (accountId: number, payee: string) =>
    invoke<number | null>("suggest_transfer_account", { accountId, payee }),
  periodSummary: (from: string, to: string) => invoke<PeriodSummary>("period_summary", { from, to }),
  listTransferCandidates: () => invoke<TransferCandidate[]>("list_transfer_candidates"),
  confirmTransferLink: (aId: number, bId: number) => invoke<void>("confirm_transfer_link", { aId, bId }),
  dismissTransferCandidate: (aId: number, bId: number) => invoke<void>("dismiss_transfer_candidate", { aId, bId }),
  unlinkTransfer: (id: number) => invoke<void>("unlink_transfer", { id }),

  // CSV import
  importDetect: (accountId: number, text: string) => invoke<Detection>("import_detect", { accountId, text }),
  importPreview: (accountId: number, text: string, mapping: CsvMapping) =>
    invoke<ImportPreview>("import_preview", { accountId, text, mapping }),
  importCommit: (
    accountId: number,
    text: string,
    mapping: CsvMapping,
    lines: number[],
    categoryOverrides: Record<number, number | null>,
  ) => invoke<ImportResult>("import_commit", { accountId, text, mapping, lines, categoryOverrides }),
  listImportProfiles: () => invoke<ImportProfile[]>("list_import_profiles"),
  saveImportProfile: (name: string, mapping: CsvMapping) =>
    invoke<ImportProfile>("save_import_profile", { name, mapping }),
  deleteImportProfile: (id: number) => invoke<void>("delete_import_profile", { id }),

  // Plaid
  plaidStatus: () => invoke<PlaidStatus>("plaid_status"),
  plaidSaveCredentials: (environment: string, clientId: string, secret: string) =>
    invoke<PlaidStatus>("plaid_save_credentials", { environment, clientId, secret }),
  plaidClearCredentials: (environment?: string) =>
    invoke<PlaidStatus>("plaid_clear_credentials", { environment: environment ?? null }),
  /** Switching to Production needs the master password once every 10 minutes. */
  plaidSetEnvironment: (environment: string, password?: string) =>
    invoke<PlaidStatus>("plaid_set_environment", { environment, password: password ?? null }),
  plaidListItems: () => invoke<PlaidItem[]>("plaid_list_items"),
  plaidStartLink: (kind: "banking" | "investments") => invoke<LinkStart>("plaid_start_link", { kind }),
  plaidStartReconnect: (itemId: number) => invoke<LinkStart>("plaid_start_reconnect", { itemId }),
  plaidReopenLink: (linkToken: string) => invoke<void>("plaid_reopen_link", { linkToken }),
  plaidPollLink: (linkToken: string) => invoke<LinkPoll>("plaid_poll_link", { linkToken }),
  plaidCancelLink: (linkToken: string) => invoke<void>("plaid_cancel_link", { linkToken }),
  plaidItemAccounts: (itemId: number) => invoke<PlaidAccountView[]>("plaid_item_accounts", { itemId }),
  plaidLinkAccounts: (itemId: number, choices: AccountChoice[]) =>
    invoke<{ summary: { created: number; linked: number; unlinked: number }; sync: SyncResult }>(
      "plaid_link_accounts",
      { itemId, choices },
    ),
  plaidSyncItem: (itemId: number) => invoke<SyncResult>("plaid_sync_item", { itemId }),
  plaidSyncAll: () => invoke<SyncOutcome[]>("plaid_sync_all"),
  plaidRemoveItem: (itemId: number, force: boolean) => invoke<void>("plaid_remove_item", { itemId, force }),
  plaidSandboxAddItem: (kind: "banking" | "investments") => invoke<LinkPoll>("plaid_sandbox_add_item", { kind }),
  listHoldings: (accountId: number) => invoke<HoldingView[]>("list_holdings", { accountId }),

  // Calculators
  calcRetirement: (input: RetirementInput) => invoke<RetirementResult>("calc_retirement", { input }),
  calcRetirementMonteCarlo: (input: MonteCarloInput) =>
    invoke<MonteCarloResult>("calc_retirement_monte_carlo", { input }),
  calcSavingsGoal: (input: SavingsGoalInput) => invoke<SavingsGoalResult>("calc_savings_goal", { input }),
  calcHomeAffordability: (input: HomeAffordabilityInput) =>
    invoke<HomeAffordabilityResult>("calc_home_affordability", { input }),
  calcRentVsBuy: (input: RentVsBuyInput) => invoke<RentVsBuyResult>("calc_rent_vs_buy", { input }),
  calcDebtPayoff: (input: DebtPayoffInput) => invoke<DebtPayoffResult>("calc_debt_payoff", { input }),
  calcBudgetPlan: (input: BudgetPlanInput) => invoke<BudgetPlanResult>("calc_budget_plan", { input }),
  listScenarios: (calculator: string) => invoke<Scenario[]>("list_scenarios", { calculator }),
  saveScenario: (calculator: string, name: string, inputs: unknown) =>
    invoke<Scenario>("save_scenario", { calculator, name, inputs }),
  deleteScenario: (id: number) => invoke<void>("delete_scenario", { id }),
};

export function onVaultLocked(cb: (reason: string) => void): Promise<UnlistenFn> {
  return listen<string>("vault-locked", (e) => cb(e.payload));
}
