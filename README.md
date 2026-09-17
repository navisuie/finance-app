# Finance

A personal finance tracker for macOS. Your data stays on your Mac in an encrypted database. There is no server and no account to sign up for.

**Status: phase 2 of 6.** Done so far:

- **Phase 1:** the vault, lock screen, auto-lock and recovery key.
- **Phase 2:** accounts, balance history, net worth, transactions, categories and CSV import.

Budgets, calculators and Plaid bank sync come in later phases.

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
cargo test -p finance-core   # core, security, import tests (53)
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
* **Locked-down webview.** A strict Content Security Policy allows no remote scripts and no network access from the UI. The webview has no permissions for the filesystem, shell, or HTTP. All sensitive work runs in Rust commands, and every data command refuses to run while the vault is locked.
* **Minimal data.** Store only the last 4 digits of account numbers. The schema enforces this.

### Your part

* Turn on **FileVault** (System Settings → Privacy & Security).
* Use a strong macOS login password and lock your screen when you step away.
* Keep the recovery key offline, for example on paper or in a password manager.

## Project layout

```
crates/finance-core/   Rust: vault, crypto, schema, accounts, transactions, CSV import (unit-tested)
src-tauri/             Rust: Tauri shell, IPC commands, auto-lock timer
src/                   React + TypeScript UI (Tailwind)
```

## Roadmap

1. **Secure foundation** ✅
2. **Accounts and transactions (manual and CSV import)** ✅
3. Dashboard, budgets, goals
4. Calculators (retirement, Monte Carlo, savings goal, home affordability, rent vs buy, debt payoff)
5. Touch ID unlock, encrypted backups, signed build
6. **Live bank sync with Plaid.** Plaid Hosted Link opens in your browser. The client secret is kept in the macOS Keychain, and access tokens stay inside the encrypted database.
