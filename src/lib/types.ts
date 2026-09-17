// Mirrors the Rust types in crates/finance-core (serde camelCase).

export type AccountKind =
  | "checking"
  | "savings"
  | "hysa"
  | "brokerage"
  | "401k"
  | "roth_ira"
  | "traditional_ira"
  | "hsa"
  | "credit_card"
  | "loan"
  | "mortgage"
  | "other";

export interface Account {
  id: number;
  name: string;
  kind: AccountKind;
  institution: string | null;
  last4: string | null;
  isAsset: boolean;
  archived: boolean;
  linked: boolean;
  /** Created by a Plaid Sandbox connection: test data. */
  isTest: boolean;
  balanceCents: number | null;
  balanceAsOf: string | null;
  transactionCount: number;
}

export interface AccountInput {
  name: string;
  kind: AccountKind;
  institution?: string | null;
  last4?: string | null;
  isAsset?: boolean | null;
  balanceCents?: number | null;
  balanceAsOf?: string | null;
}

export interface BalanceSnapshot {
  id: number;
  accountId: number;
  asOf: string;
  amountCents: number;
  source: string;
}

export interface NetWorth {
  assetsCents: number;
  liabilitiesCents: number;
  netCents: number;
}

export type CategoryKind = "income" | "expense" | "transfer";

export interface Category {
  id: number;
  name: string;
  kind: CategoryKind;
  transactionCount: number;
}

export interface Transaction {
  id: number;
  accountId: number;
  accountName: string;
  postedOn: string;
  amountCents: number;
  payee: string;
  categoryId: number | null;
  categoryName: string | null;
  notes: string | null;
  pending: boolean;
  source: "manual" | "csv" | "plaid" | string;
}

export interface TransactionInput {
  accountId: number;
  postedOn: string;
  amountCents: number;
  payee: string;
  categoryId: number | null;
  notes: string | null;
}

/** categoryId 0 = uncategorized */
export interface TransactionFilter {
  accountId?: number;
  categoryId?: number;
  from?: string;
  to?: string;
  search?: string;
  /** Defaults to the active environment's data (test in Sandbox, real in Production). */
  scope?: Scope;
  limit?: number;
  offset?: number;
}

export interface TransactionPage {
  items: Transaction[];
  total: number;
  inflowCents: number;
  outflowCents: number;
}

export interface PeriodSummary {
  incomeCents: number;
  spendingCents: number;
  transactionCount: number;
}

export type AmountMapping =
  | { mode: "single"; column: number; negate: boolean }
  | { mode: "split"; debitColumn: number; creditColumn: number };

export interface CsvMapping {
  skipRows: number;
  headerRow: number | null;
  dateColumn: number;
  dateFormat: string;
  payeeColumn: number;
  amount: AmountMapping;
  categoryColumn: number | null;
  notesColumn: number | null;
}

export interface Detection {
  columns: string[];
  sample: string[][];
  dataRowCount: number;
  mapping: CsvMapping | null;
  fromPreviousImport: boolean;
  dateFormats: { format: string; example: string }[];
}

export type RowStatus = "new" | "duplicate" | "possible_duplicate" | "error";

export interface PreviewRow {
  line: number;
  postedOn: string | null;
  amountCents: number | null;
  payee: string;
  categoryId: number | null;
  categoryName: string | null;
  notes: string | null;
  status: RowStatus;
  message: string | null;
}

export interface ImportPreview {
  rows: PreviewRow[];
  newCount: number;
  duplicateCount: number;
  possibleDuplicateCount: number;
  errorCount: number;
  firstDate: string | null;
  lastDate: string | null;
}

export interface ImportResult {
  inserted: number;
  skipped: number;
}

export interface ImportProfile {
  id: number;
  name: string;
  mapping: CsvMapping;
}

export interface CategoryInput {
  name: string;
  kind: CategoryKind;
}

// ---- Plaid

export type PlaidEnvironment = "sandbox" | "production";

/** Which set of data a query covers. */
export type Scope = "real" | "test" | "all";

export interface TestDataSummary {
  accounts: number;
  transactions: number;
}

export interface EnvironmentStatus {
  environment: PlaidEnvironment;
  configured: boolean;
  clientIdHint: string | null;
}

export interface PlaidStatus {
  /** Whether the active environment has keys. */
  configured: boolean;
  environment: PlaidEnvironment;
  clientIdHint: string | null;
  environments: EnvironmentStatus[];
}

export interface PlaidItem {
  id: number;
  institutionName: string;
  status: "active" | "needs_reauth" | "error" | string;
  lastError: string | null;
  lastSyncedAt: string | null;
  products: string[];
  environment: PlaidEnvironment | null;
  linkedAccountCount: number;
  createdAt: string;
}

export interface LinkStart {
  linkToken: string;
  url: string;
  expiration: string | null;
}

export type LinkPoll =
  | { state: "waiting" }
  | { state: "exited"; message: string }
  | { state: "connected"; itemId: number; institutionName: string }
  | { state: "reconnected"; itemId: number };

export interface PlaidAccountView {
  plaidAccountId: string;
  name: string;
  officialName: string | null;
  mask: string | null;
  accountType: string;
  subtype: string | null;
  suggestedKind: AccountKind;
  balanceCents: number | null;
  linkedAccountId: number | null;
  suggestedMatchId: number | null;
  suggestedSyncFrom: string | null;
}

export interface AccountChoice {
  plaidAccountId: string;
  action: "create" | "link" | "skip";
  accountId?: number | null;
  kind?: AccountKind | null;
  name?: string | null;
  syncFrom?: string | null;
}

export interface SyncResult {
  itemId: number;
  institutionName: string;
  transactions: { added: number; modified: number; removed: number; skipped: number };
  balancesUpdated: number;
  holdingsUpdated: number;
  historyPending: boolean;
}

export interface SyncOutcome {
  itemId: number;
  result: SyncResult | null;
  error: string | null;
}

export interface HoldingView {
  ticker: string | null;
  name: string;
  securityType: string | null;
  quantity: number;
  valueCents: number;
  costBasisCents: number | null;
  asOf: string;
}
