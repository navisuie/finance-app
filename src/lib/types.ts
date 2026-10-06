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
  /** Annual rate in basis points (6.49% = 649), for debts. */
  aprBps: number | null;
  monthlyPaymentCents: number | null;
}

export interface AccountInput {
  name: string;
  kind: AccountKind;
  institution?: string | null;
  last4?: string | null;
  isAsset?: boolean | null;
  balanceCents?: number | null;
  balanceAsOf?: string | null;
  aprBps?: number | null;
  monthlyPaymentCents?: number | null;
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
  /** Set when the money went to another account you track. */
  transferAccountId: number | null;
  transferAccountName: string | null;
  /** Set once this is confirmed as one leg of an auto-matched transfer. */
  linkedTransactionId: number | null;
}

export interface TransferCandidate {
  a: Transaction;
  b: Transaction;
}

export interface TransactionInput {
  accountId: number;
  postedOn: string;
  amountCents: number;
  payee: string;
  categoryId: number | null;
  notes: string | null;
  transferAccountId?: number | null;
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
  /** Money moved into savings, brokerage or retirement accounts. */
  savedCents: number;
  /** How much of spendingCents has no category — the usual reason a rate looks wrong. */
  uncategorizedSpendingCents: number;
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

export interface Payoff {
  months: number;
  payoffOn: string;
  totalInterestCents: number;
  totalPaidCents: number;
  lastPaymentCents: number;
}

export type Projection =
  | { state: "clear" }
  | { state: "incomplete" }
  | { state: "neverPaidOff"; monthlyInterestCents: number; minimumToMakeProgressCents: number }
  | ({ state: "paid" } & Payoff);

export interface PaymentsSince {
  since: string | null;
  count: number;
  totalCents: number;
}

export interface LinkedPayment {
  id: number;
  postedOn: string;
  /** Positive: what the payment was worth. */
  amountCents: number;
  payee: string;
  fromAccountId: number;
  fromAccountName: string;
}

export interface DebtSummary {
  account: Account;
  aprBps: number | null;
  monthlyPaymentCents: number | null;
  payments: PaymentsSince;
  /** Payments tagged as paying this debt, newest first. */
  recentPayments: LinkedPayment[];
  /** True when the app keeps this balance up to date from tagged payments. */
  autoTracked: boolean;
  /** The statement balance the running total is counted from. */
  anchoredOn: string | null;
  estimatedInterestCents: number;
  suggestedBalanceCents: number | null;
  projection: Projection;
}

export interface NetWorthPoint {
  month: string;
  assetsCents: number;
  liabilitiesCents: number;
  netCents: number;
  hasData: boolean;
}

export type BudgetState = "under" | "close" | "over";

export interface BudgetRow {
  categoryId: number;
  categoryName: string;
  limitCents: number | null;
  spentCents: number;
  remainingCents: number | null;
  used: number | null;
  state: BudgetState | null;
  recentAverageCents: number;
}

export interface BudgetMonth {
  month: string;
  rows: BudgetRow[];
  totalLimitCents: number;
  totalSpentCents: number;
  unbudgetedSpentCents: number;
  incomeCents: number;
}

export interface FixedPayment {
  accountId: number;
  accountName: string;
  monthlyPaymentCents: number;
}

export interface PlanLine {
  categoryId: number;
  categoryName: string;
  suggestedCents: number;
  recentAverageCents: number;
  currentLimitCents: number | null;
}

/** A suggested budget derived from the months before it. */
export interface BudgetDraft {
  month: string;
  monthsOfData: number;
  monthlyIncomeCents: number;
  savingInvestingCents: number;
  debtPayments: FixedPayment[];
  debtPaymentsCents: number;
  lines: PlanLine[];
  suggestedTotalCents: number;
  leftoverCents: number;
}

/** What a month's income looks like against what you usually bring in. */
export interface IncomeBaseline {
  month: string;
  typicalMonthlyCents: number;
  monthsOfData: number;
  receivedCents: number;
  /** What to measure a savings rate against. */
  expectedCents: number;
  /** True when expectedCents is the typical figure, not what has landed. */
  estimated: boolean;
}

export interface LimitInput {
  categoryId: number;
  limitCents: number;
}

export interface Goal {
  id: number;
  name: string;
  targetCents: number;
  targetDate: string | null;
  accountId: number | null;
  accountName: string | null;
  currentCents: number;
  remainingCents: number;
  progress: number;
  reached: boolean;
  monthsLeft: number | null;
  monthlyNeededCents: number | null;
  overdue: boolean;
}

export interface GoalInput {
  name: string;
  targetCents: number;
  targetDate: string | null;
  accountId: number | null;
}

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

// ---------------------------------------------------------------- calculators

export interface RetirementInput {
  currentAge: number;
  retirementAge: number;
  balanceCents: number;
  monthlyContributionCents: number;
  employerMatchPct: number;
  expectedReturnPct: number;
  inflationPct: number;
  desiredAnnualIncomeCents: number;
  withdrawalRatePct: number;
}

export interface RetirementResult {
  monthsToRetirement: number;
  combinedMonthlyContributionCents: number;
  projectedBalanceCents: number;
  sustainableAnnualIncomeCents: number;
  gapCents: number;
  requiredMonthlySavingsCents: number;
  balanceByYearCents: number[];
}

export interface MonteCarloInput extends RetirementInput {
  returnVolatilityPct: number;
}

export interface MonteCarloResult {
  successProbabilityPct: number;
  p10ByYearCents: number[];
  p50ByYearCents: number[];
  p90ByYearCents: number[];
}

export interface SavingsGoalInput {
  targetCents: number;
  targetDate: string;
  currentSavingsCents: number;
  hysaApyPct: number;
  currentMonthlyDepositCents: number;
}

export interface SavingsGoalResult {
  monthsToTargetDate: number;
  requiredMonthlyDepositCents: number;
  projectedCompletionDate: string | null;
  alreadyReached: boolean;
}

export interface HomeAffordabilityInput {
  grossAnnualIncomeCents: number;
  monthlyDebtsCents: number;
  downPaymentCents: number;
  mortgageRatePct: number;
  termYears: number;
  propertyTaxPct: number;
  monthlyInsuranceCents: number;
  monthlyHoaCents: number;
}

export interface PaymentBreakdown {
  principalInterestCents: number;
  taxCents: number;
  insuranceCents: number;
  hoaCents: number;
  pmiCents: number;
  totalCents: number;
}

export interface HomeAffordabilityResult {
  maxHomePriceCents: number;
  maxLoanCents: number;
  monthlyPayment: PaymentBreakdown;
  downPaymentPct: number;
}

export interface RentVsBuyInput {
  monthlyRentCents: number;
  annualRentGrowthPct: number;
  homePriceCents: number;
  downPaymentCents: number;
  mortgageRatePct: number;
  termYears: number;
  maintenancePct: number;
  annualAppreciationPct: number;
  yearsStaying: number;
  investmentReturnPct: number;
}

export interface YearlyWealth {
  year: number;
  rentNetWorthCents: number;
  buyNetWorthCents: number;
}

export interface RentVsBuyResult {
  byYear: YearlyWealth[];
  breakEvenYear: number | null;
}

export interface DebtInput {
  name: string;
  balanceCents: number;
  aprBps: number;
  minimumPaymentCents: number;
}

export interface DebtPayoffInput {
  debts: DebtInput[];
  extraMonthlyCents: number;
}

export interface StrategyResult {
  months: number;
  payoffOn: string;
  totalInterestCents: number;
  totalPaidCents: number;
  order: string[];
}

export interface DebtPayoffResult {
  avalanche: StrategyResult;
  snowball: StrategyResult;
}

export type Bucket = "needs" | "wants" | "savings";

export interface FixedExpense {
  label: string;
  monthlyCents: number;
  bucket: Bucket;
  /** The suggested category this replaces, if any. */
  covers: string | null;
}

export interface BudgetPlanInput {
  grossAnnualIncomeCents: number;
  taxRatePct: number;
  fixedExpenses: FixedExpense[];
  needsPct: number;
  wantsPct: number;
  savingsPct: number;
}

export interface BucketSummary {
  bucket: Bucket;
  targetCents: number;
  fixedCents: number;
  remainingCents: number;
}

export interface BudgetPlanLine {
  bucket: Bucket;
  label: string;
  monthlyCents: number;
  source: "fixed" | "suggested";
  /** The spending category this line would be budgeted under, if any. */
  category: string | null;
}

export interface BudgetPlanResult {
  monthlyTakeHomeCents: number;
  buckets: BucketSummary[];
  lines: BudgetPlanLine[];
  totalPlannedCents: number;
  unallocatedCents: number;
  overAllocated: boolean;
}

export type CalculatorId =
  | "retirement"
  | "monte_carlo"
  | "savings_goal"
  | "home"
  | "rent_vs_buy"
  | "debt_payoff"
  | "budget_plan";

export interface Scenario {
  id: number;
  calculator: string;
  name: string;
  inputs: unknown;
  createdAt: string;
}
