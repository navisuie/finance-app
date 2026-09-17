import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Account,
  AccountInput,
  BalanceSnapshot,
  Category,
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
    | "json";
  message: string;
  retry_after_secs?: number;
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

  // Accounts
  listAccounts: (includeArchived = false) => invoke<Account[]>("list_accounts", { includeArchived }),
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
  periodSummary: (from: string, to: string) => invoke<PeriodSummary>("period_summary", { from, to }),

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
};

export function onVaultLocked(cb: (reason: string) => void): Promise<UnlistenFn> {
  return listen<string>("vault-locked", (e) => cb(e.payload));
}
