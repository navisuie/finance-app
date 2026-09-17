# Finance

A personal finance tracker for macOS. Your data stays on your Mac in an encrypted database. There is no server and no account to sign up for.

**Status: phases 1, 2 and 6 are done.**

- **Phase 1:** the vault, lock screen, auto-lock and recovery key.
- **Phase 2:** accounts, balance history, net worth, transactions, categories and CSV import.
- **Phase 6:** live bank sync through Plaid.

Budgets, goals, calculators, Touch ID and backups come next.

## Setup (one time)

Run these in Terminal:

```bash
# 1. Apple command-line tools (compiler, git)
xcode-select --install

# 2. Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 3. Node.js 20 or newer (skip this if `node -v` already shows v20+)
#    Installer: https://nodejs.org, or with Homebrew: brew install node

# 4. Project dependencies
cd ~/repos/finance-app
npm install
```

## Run

```bash
npm run app          # development build, opens the app window
npm test             # frontend unit tests (21)
cargo test -p finance-core   # core, security, import and Plaid tests
npm run app:build    # release build -> src-tauri/../target/release/bundle/
```

The first build takes several minutes because it compiles SQLCipher and OpenSSL from source. Later builds are fast.

To regenerate the full macOS icon set from `app-icon.png`, run `npx tauri icon app-icon.png`.

## Using it

* **Accounts.** Add each account with its current balance. Debts (credit cards, loans, mortgage) are entered as the positive amount you owe. Net worth is assets minus debts, based on each account's latest balance. Use **Update balance** on an account's page each month to build a history.
* **Transactions.** Negative amounts are money out and positive amounts are money in. Adding or importing transactions does not change an account's balance; the balance you enter is treated as the source of truth.
* **Categories.** Categories have a type: Spending, Income or Transfer. Transfers, such as moving money to your HYSA or paying a card, are left out of spending and income. When you enter a payee you've used before, the app suggests the category you used last time.
* **CSV import.** Export a CSV from your bank's website, then use **Import CSV**.
    - **Columns.** The app detects the columns, including files that have extra lines at the top, separate debit and credit columns, or card exports where purchases are positive numbers. You can correct any column choice before importing.
    - **Duplicates.** Rows you've already imported are skipped. Rows that match a transaction you entered by hand are flagged and left unchecked.
    - **Saved settings.** The column choices are remembered for that account next time. You can also save them as a named profile.
    - **Privacy.** The file is read on your Mac and never uploaded.
* **Hide balances** (bottom of the sidebar) replaces every amount with •••••.

## Connecting banks with Plaid

### Setup

1. Create a free account at [dashboard.plaid.com](https://dashboard.plaid.com). On the free Trial plan you can connect up to **10 institutions** in Production. Removing a connection does not give the slot back, so connect deliberately.
2. In the Plaid Dashboard, open **Developers → Keys**. Copy your **client ID** and a **secret**. Each environment has its own secret.
3. In the app, open **Settings → Bank connections**. Pick the environment, paste both keys, and click **Verify and save**. The app checks the keys with Plaid before storing them.

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
- **Test vs real connections.** Connections made with Sandbox keys and with Production keys are kept separate. A test connection can't be synced with Production keys, and the reverse.

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
* **Locked-down webview.** A strict Content Security Policy allows no remote scripts and no network access from the UI (Plaid calls are made by the Rust backend). The webview has no permissions for the filesystem, shell, or HTTP. All sensitive work runs in Rust commands, and every data command refuses to run while the vault is locked.
* **Minimal data.** Store only the last 4 digits of account numbers. The schema enforces this.

### Your part

* Turn on **FileVault** (System Settings → Privacy & Security).
* Use a strong macOS login password and lock your screen when you step away.
* Keep the recovery key offline, for example on paper or in a password manager.

## Project layout

```
crates/finance-core/   Rust: vault, crypto, schema, accounts, transactions, CSV import, Plaid (unit-tested)
src-tauri/             Rust: Tauri shell, IPC commands, auto-lock timer
src/                   React + TypeScript UI (Tailwind)
```

## Roadmap

1. **Secure foundation** ✅
2. **Accounts and transactions (manual and CSV import)** ✅
3. Dashboard, budgets, goals
4. Calculators (retirement, Monte Carlo, savings goal, home affordability, rent vs buy, debt payoff)
5. Touch ID unlock, encrypted backups, signed build
6. **Live bank sync with Plaid** ✅
