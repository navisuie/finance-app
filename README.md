# Finance

A private, local-first personal finance app for macOS.

Track your accounts, spending, budgets, debts and goals, run "what if" calculators, and optionally sync your banks through Plaid — all without a server, a subscription or an account. Everything lives in an encrypted database on your own Mac.

Built with [Tauri 2](https://tauri.app) (Rust) and React + TypeScript.

> **Status: personal project, early and unsigned.** It works and I use it daily, but there are no prebuilt downloads yet — you build it from source. It's macOS-only, it hasn't been security-audited by anyone else, and the build is not signed or notarized by Apple. Treat it accordingly, and keep your recovery key safe: if you lose both your password and your recovery key, your data cannot be recovered.

## What it does

- **Encrypted vault.** All data sits in a SQLCipher (AES-256) database unlocked by your master password, with a one-time recovery key as a backup. Auto-locks when idle.
- **Accounts and net worth.** Bank accounts, cards, loans, mortgages and investments, with balance history and a net-worth-over-time chart.
- **Transactions.** Add them by hand, import any bank's CSV export (columns are auto-detected), or sync them from your bank. Categories are learned from payees you've used before.
- **Transfers.** Moving money between your own accounts is kept out of spending. The app spots likely transfer pairs and asks you to confirm them.
- **Budgets.** Monthly limits per category, a budget builder that suggests limits from your last three months, and a savings/investing target.
- **Debts.** Payoff date, payments left and interest to come for each loan; hand-entered loans update their own balance as you pay them.
- **Goals.** Savings targets with optional dates, optionally tracking a linked account's balance.
- **Calculators.** Retirement (straight-line and Monte Carlo), savings goal, home affordability, rent vs. buy, debt payoff (avalanche vs. snowball) and a budget-from-salary planner. Save named scenarios.
- **Bank sync (optional).** Bring your own free [Plaid](https://plaid.com) keys. Try it with Plaid's fake Sandbox banks first; Sandbox data is kept completely separate from real data.
- **Encrypted backups.** Export a single password-protected backup file and restore it on a fresh install.

## Try it

You need a Mac. Plaid is optional — the app is fully usable with manual entry and CSV import.

### 1. Install the tools (one time)

```bash
# Apple command-line tools (compiler, git)
xcode-select --install

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Node.js 20 or newer (skip if `node -v` already shows v20+)
# Installer: https://nodejs.org, or with Homebrew: brew install node
```

### 2. Get the code and run it

```bash
git clone https://github.com/navisuie/finance-app.git
cd finance-app
npm install
npm run app          # development build, opens the app window
```

The first build takes several minutes because it compiles SQLCipher and OpenSSL from source. Later builds are fast.

On first launch you create a master password and are shown a **recovery key** once — write it down or put it in a password manager.

### 3. Build a standalone app (optional)

```bash
npm run app:build    # -> target/release/bundle/macos/Finance.app (and a .dmg)
```

Because the build isn't signed with an Apple Developer ID, macOS may block it the first time. Right-click the app → **Open**, or allow it under **System Settings → Privacy & Security**.

### Tests

```bash
npm test                                          # frontend unit tests
cargo test -p finance-core                        # core, security, import, calculator and Plaid tests
cargo clippy --workspace --all-targets -- -D warnings
npm run audit                                     # npm + cargo dependency audit (needs cargo-audit)
```

To regenerate the macOS icon set from `app-icon.png`, run `npx tauri icon app-icon.png`.

## Using it

* **Accounts.** Add each account with its current balance. Debts (credit cards, loans, mortgage) are entered as the positive amount you owe. Net worth is assets minus debts, based on each account's latest balance. Use **Update balance** on an account's page each month to build a history.
* **Transactions.** Negative amounts are money out and positive amounts are money in. Adding or importing transactions does not change an account's balance; the balance you enter is treated as the source of truth.
* **Categories.** Categories have a type: Spending, Income or Transfer. Transfers, such as moving money to your HYSA or paying a card, are left out of spending and income. When you enter a payee you've used before, the app suggests the category you used last time.
* **CSV import.** Export a CSV from your bank's website, then use **Import CSV**.
    - **Columns.** The app detects the columns, including files that have extra lines at the top, separate debit and credit columns, or card exports where purchases are positive numbers. You can correct any column choice before importing.
    - **Duplicates.** Rows you've already imported are skipped. Rows that match a transaction you entered by hand are flagged and left unchecked.
    - **Saved settings.** The column choices are remembered for that account next time. You can also save them as a named profile.
    - **Privacy.** The file is read on your Mac and never uploaded.
* **Transfers between your accounts.** Mark a transaction as **Paid to another account** (a card payment, a loan payment, a move to savings) and it's left out of spending and income. The same payee from the same account is tagged automatically next time. The app also looks for matching pairs — the same amount leaving one account and arriving in another within 3 days — and lists them under **Possible transfers** for you to link or dismiss. It never links them on its own.
* **Hide balances** (bottom of the sidebar) replaces every amount with •••••.

## Debts you track yourself

Not every debt comes from a bank connection. Add a car loan, a personal loan or a mortgage with **Add account → Loan** (or Mortgage), and enter what you owe as a positive number. It counts against your net worth straight away.

* **Terms.** A debt account can hold its interest rate and your usual monthly payment. With both, its page shows when it will be paid off, how many payments are left, and the interest still to come. If the payment only covers the interest, it says so and tells you the amount that would start reducing the balance.
* **Linking payments.** On the transaction that leaves your checking account, set **Paid to another account** to the loan. That payment then counts as moving money between your own accounts, not as spending, so it stays out of your spending total and savings rate. It works for credit card payments and transfers to savings too.
* **The balance goes down as you pay.** For a debt you entered yourself that has an interest rate, the balance updates on its own: your last statement balance, plus estimated interest since then, minus the payments linked to it. Enter a new statement balance with **Update balance** whenever you have one and the estimate restarts from there. Debts synced from a bank always show the bank's figure.

## Budgets, goals and net worth

* **Budgets.** One monthly limit per spending category. The arrows at the top move between months (you can't look past the current month). Each category shows what you've spent, what's left and a bar; it turns amber at 85% of the limit and red once you're over, with the words next to it so it doesn't rely on colour alone. Categories without a limit are listed too, with their average over the last three months and a **Use average** button that turns it into a limit. Click a limit to edit it, Enter or click away to save, Escape to cancel, and set it to 0 to remove it. **Copy last month's budgets** fills in a fresh month, or **Build a budget** drafts one from what actually happened — income and saving from your accounts, debt payments from your loans, and each category's recent average — for you to adjust before applying.
* **Saving & investing.** Set a monthly target for money moved into savings and investment accounts. Going over is the good outcome, so the bar never turns red. Transfers and payments linked to another account are never counted as spending.
* **Goals.** A goal has a name, a target amount and, optionally, a date and a linked account. Linked to an account, progress is that account's balance, so it keeps itself up to date; otherwise you set the amount saved yourself. With a date, the card shows what you'd need to put aside each month, or says how far past the date you are. Goals attached to a test account only appear in Sandbox.
* **Net worth over time.** The dashboard charts the last twelve months from your balance history — every balance you record, whether by hand, by CSV or from a bank sync, adds a point. Hover for the month's figure, or open **Show the numbers** for the assets, debts and net worth behind each point. Closed accounts still count for the months they were open. The chart follows **Hide balances** and shows sandbox data in Sandbox.

## Calculators

The **Calculators** page has seven planners. They're pure maths on the numbers you type in — nothing is saved to your accounts — but you can save named scenarios and load them back later.

- **Retirement projection** — whether your savings support the retirement you want.
- **Monte Carlo retirement** — the odds your money lasts, across 2,000 simulated markets, with a median line and percentile band.
- **Savings goal** — what to set aside monthly to hit a target by a date.
- **Home affordability** — the maximum home price under the 28/36 rule.
- **Rent vs. buy** — net wealth under each path, year by year.
- **Debt payoff** — avalanche vs. snowball, payoff date and total interest.
- **Budget from a salary** — give a salary and the costs you know; it fills in the rest.

These are estimates for planning, not financial advice.

## Backups

**Settings → Backups** exports a single encrypted backup file (your password is required, and the file is sealed with AES-256-GCM under a key derived from it). To restore, start the app on a Mac with no vault and choose **Restore from a backup instead** on the setup screen. Restore never overwrites an existing vault.

## Connecting banks with Plaid

### Setup

1. Create a free account at [dashboard.plaid.com](https://dashboard.plaid.com). On the free Trial plan you can connect up to **10 institutions** in Production. Removing a connection does not give the slot back, so connect deliberately.
2. In the Plaid Dashboard, open **Developers → Keys**. Copy your **client ID** and a **secret**. Each environment has its own secret.
3. In the app, open **Settings → Bank connections**. Pick the environment, paste both keys, and click **Verify and save**. The app checks the keys with Plaid before storing them.
4. Sandbox and Production keys are stored separately, so you can save both and switch whenever you like.

### Switching between test and real banks

The **Sandbox / Production** switch sits next to **Bank connections** on the Accounts page (and in Settings). Switching is instant and never asks for keys again.

Connections belong to the environment they were made in: while Sandbox is active you see and sync your test banks, and your real ones are greyed out with a note, and the reverse. Nothing is deleted by switching.

### Guardrails around live data

* **Sandbox is a separate world.** Accounts created by a Sandbox connection are marked as test data. While Sandbox is active, every list and total — accounts, net worth, assets, debts, transactions, spending — covers that test data only, so you can try the whole app with fake banks. Switch to Production and your real accounts and numbers are back, with the test data out of sight. **Settings → Delete test data** removes it all at once without touching real accounts.
* **Test and real data can't be mixed.** A Sandbox connection can only be linked to a test account, and a real bank only to a real account. The app refuses anything else.
* **You always know which mode you're in.** The sidebar shows a SANDBOX or LIVE tag, Sandbox adds a banner across the top, and the window title reads "Finance — SANDBOX (test data)".
* **Going live needs your password.** Switching to Production, and connecting or reconnecting a bank there, asks for your master password. One confirmation covers the next 10 minutes of live actions and keeps sliding while you're working, and locking the vault clears it. If it does expire in the middle of something, the password box appears right where you are and the action carries on once you've entered it.
* **Connecting a real bank says so.** The Connect dialog states plainly that it uses one of the 10 institutions on Plaid's free Trial plan and that disconnecting doesn't give the slot back.

### Try Sandbox first

With the Sandbox keys, go to **Accounts → Connect a bank**. Either:

- click **Add test bank**, which creates a test connection instantly, or
- choose a bank in the browser and sign in with `user_good` / `pass_good`.

### Connecting your real accounts

Switch to your **Production** secret in Settings, then use **Connect a bank**:

- **Bank or credit card** syncs balances and transactions (up to 2 years of history).
- **Investments or retirement** syncs balances and holdings for brokerage accounts, 401(k)s and IRAs.

Plaid opens in your default browser. You sign in to your bank on Plaid's site, so your bank password never passes through this app. When you finish, the app picks up the connection automatically.

Next you choose what to do with each account: **add** it as a new account, **link** it to an account you already track, or **skip** it. When you link an existing account, Plaid history starts the day after your latest existing transaction, so nothing gets duplicated.

### Syncing

- Use **Sync** or **Sync all** on the Accounts page.
- After you unlock, the app also syncs in the background if the data is more than 6 hours old.
- A background sync doesn't reset the auto-lock timer.

### What sync does

- **Balances:** each account gets today's balance.
- **Transactions:** new, changed and removed transactions are applied.
- **Categories:**
    - If you've categorized that payee before, your choice is used.
    - Otherwise the app uses Plaid's category.
    - When a pending charge posts, it keeps the category you gave the pending one.
- **Holdings:** investment holdings are replaced with the latest.

### Problems and disconnecting

- **Sign-in needed.** If a bank asks you to sign in again, the connection shows **Reconnect**.
- **Disconnect** revokes Plaid's access to that bank. Its accounts and their history stay in the app as manual accounts.

### How it's secured

- **Stored keys.** The Plaid client ID and secret, and each bank's access token, are stored inside the encrypted vault, the same AES-256 database as the rest of your data. They exist in memory only while the vault is unlocked, and the secret is never sent to the app's interface.
- **Where the app connects.** All Plaid calls are made by the Rust backend over HTTPS, and only to `sandbox.plaid.com` or `production.plaid.com`. Redirects are never followed. The interface itself still has no network access.
- **Opening the browser.** The app opens a Plaid link in your browser only after checking that it is an `https://…plaid.com` address.
- **Auto-lock during sync.** Network calls are never made while the vault is held open. If the vault auto-locks during a sync, nothing is saved.
- **Test vs real connections.** Sandbox and Production keys live in separate slots, and each connection is tied to the environment it was made in. A test connection can't be synced with Production keys, and the reverse.

## Where your data lives

`~/Library/Application Support/com.navi.financeapp/`

| File | Contents |
| --- | --- |
| `vault.db` | SQLCipher database (AES-256). Unreadable without your key. |
| `vault.json` | Two wrapped copies of the database key: one unlocked by your password, one by your recovery key. |

Both files are created with owner-only permissions (0600). **Deleting this folder erases everything.** Nothing is stored anywhere else.

## How the security works

```
master password ──Argon2id──▶ KEK₁ ──AES-256-GCM unwrap──┐
                                                          ├──▶ data key ──▶ SQLCipher (vault.db)
recovery key ────Argon2id──▶ KEK₂ ──AES-256-GCM unwrap──┘
```

* **Envelope encryption.** A random 256-bit data key encrypts the database. It is never written to disk in the clear. It is stored only after being encrypted ("wrapped") under keys derived from your password and your recovery key.
* **Argon2id** (256 MiB, 3 passes) turns your password into a key. Each guess costs about a second and a large amount of memory, which makes brute force expensive. Parameters are stored per key slot and can be raised later.
* **Wrong-password throttling.** After 3 failed attempts, the wait between attempts doubles, starting at 5 seconds and capped at 5 minutes.
* **Recovery key.** 256 random bits, shown once, in 13 groups of 4 characters. Using it resets your password and issues a new recovery key; the old one stops working.
* **Changing your password** re-wraps the data key only. It is instant and the recovery key keeps working.
* **Auto-lock is enforced by the Rust backend**, not just the UI. After the idle period (5 minutes by default, adjustable from 1 to 60), the database connection is closed and SQLCipher wipes its key from memory. The app also locks when the window closes.
* **Locked-down webview.** A strict Content Security Policy allows no remote scripts and no network access from the UI (Plaid calls are made by the Rust backend). The webview has no permissions for the filesystem, shell, or HTTP — backup files are read and written by Rust, the UI only gets the path from a native file dialog. All sensitive work runs in Rust commands, and every data command refuses to run while the vault is locked.
* **Minimal data.** Store only the last 4 digits of account numbers. The schema enforces this.

### Your part

* Turn on **FileVault** (System Settings → Privacy & Security).
* Use a strong macOS login password and lock your screen when you step away.
* Keep the recovery key offline, for example on paper or in a password manager.

## Project layout

```
crates/finance-core/   Rust: vault, crypto, schema, accounts, transactions, budgets, goals,
                       debt maths, transfers, calculators, CSV import, Plaid (unit-tested)
src-tauri/             Rust: Tauri shell, IPC commands, auto-lock timer
src/                   React 19 + TypeScript UI (Tailwind v4, Vite)
```

## Roadmap

Done: encrypted vault, accounts and transactions, CSV import, dashboard, budgets, goals, debts, transfer detection, calculators, encrypted backups, Plaid bank sync.

Not yet:

- Touch ID unlock
- A signed and notarized build with downloadable releases
