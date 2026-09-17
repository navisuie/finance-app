//! Plaid flows. Network calls happen outside any database access: each flow
//! reads what it needs, releases the vault, talks to Plaid, then writes the
//! results in a single short database step. If the vault locks meanwhile,
//! the write fails with `Locked` and nothing is saved.

use rusqlite::Connection;
use serde::Serialize;

use super::api::{Environment, LinkTokenRequest, PlaidApi, PlaidError, SyncPage};
use super::config;
use super::store::{self, AccountChoice, LinkSummary, PlaidAccountView, SyncCounts};
use crate::error::{CoreError, Result};
use crate::util::today;

/// Institution used for Sandbox test connections ("First Platypus Bank").
pub const SANDBOX_INSTITUTION: &str = "ins_109508";
const MAX_SYNC_PAGES: usize = 100;
const HISTORY_DAYS: u32 = 730;

/// Short, passive access to the unlocked database.
pub trait VaultAccess {
    fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkMode {
    New { products: Vec<String> },
    Update { item_id: i64 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStart {
    pub link_token: String,
    pub url: String,
    pub expiration: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LinkPoll {
    Waiting,
    Exited { message: String },
    Connected { item_id: i64, institution_name: String },
    Reconnected { item_id: i64 },
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub item_id: i64,
    pub institution_name: String,
    pub transactions: SyncCounts,
    pub balances_updated: usize,
    pub holdings_updated: usize,
    /// Plaid is still loading this item's history; sync again later.
    pub history_pending: bool,
}

pub fn products_for(kind: &str) -> Result<Vec<String>> {
    match kind {
        "banking" => Ok(vec!["transactions".into()]),
        "investments" => Ok(vec!["investments".into()]),
        _ => Err(CoreError::Invalid("Unknown connection type.".into())),
    }
}

fn check_env(api: &impl PlaidApi, token: &str) -> Result<()> {
    let item_env = Environment::from_access_token(token);
    if item_env != Some(api.environment()) {
        let name = item_env.map(|e| e.as_str()).unwrap_or("another environment");
        return Err(CoreError::Invalid(format!(
            "This bank was connected in {name}. Switch Plaid to {name} in Settings to use it."
        )));
    }
    Ok(())
}

pub fn start_link(api: &impl PlaidApi, vault: &impl VaultAccess, mode: &LinkMode) -> Result<LinkStart> {
    let (user_id, access_token, products) = vault.with_conn(|c| {
        let uid = config::client_user_id(c)?;
        Ok(match mode {
            LinkMode::New { products } => (uid, None, products.clone()),
            LinkMode::Update { item_id } => {
                let s = store::item_secret(c, *item_id)?;
                (uid, Some(s.access_token), Vec::new())
            }
        })
    })?;
    if let Some(t) = &access_token {
        check_env(api, t)?;
    }
    let created = api.create_link_token(&LinkTokenRequest {
        client_user_id: user_id,
        products,
        access_token,
        days_requested: HISTORY_DAYS,
    })?;
    let url = created
        .hosted_link_url
        .ok_or_else(|| CoreError::Invalid("Plaid didn't return a Hosted Link URL.".into()))?;
    let url = super::api::validate_link_url(&url)?;
    Ok(LinkStart { link_token: created.link_token, url, expiration: created.expiration })
}

pub fn poll_link(
    api: &impl PlaidApi,
    vault: &impl VaultAccess,
    link_token: &str,
    mode: &LinkMode,
) -> Result<LinkPoll> {
    let info = api.get_link_token(link_token)?;
    // Newest session last; look at all of them.
    let success = info
        .link_sessions
        .iter()
        .flat_map(|s| s.results.iter().flat_map(|r| r.item_add_results.iter()))
        .next()
        .cloned();
    let finished = info.link_sessions.iter().rev().find(|s| s.finished_at.is_some());

    match (mode, success) {
        (LinkMode::New { products }, Some(result)) => {
            let exchange = api.exchange_public_token(&result.public_token)?;
            let inst = result.institution.clone().unwrap_or_default();
            let name = match inst.name.clone() {
                Some(n) => n,
                None => api
                    .get_item(&exchange.access_token)
                    .ok()
                    .and_then(|i| i.institution_name.or(i.institution_id))
                    .unwrap_or_else(|| "Bank".into()),
            };
            let item_id = vault.with_conn(|c| {
                store::upsert_item(
                    c,
                    &exchange.item_id,
                    &exchange.access_token,
                    inst.institution_id.as_deref(),
                    Some(&name),
                    products,
                )
            })?;
            Ok(LinkPoll::Connected { item_id, institution_name: name })
        }
        (LinkMode::Update { item_id }, Some(_)) => {
            vault.with_conn(|c| store::set_status(c, *item_id, store::STATUS_ACTIVE, None))?;
            Ok(LinkPoll::Reconnected { item_id: *item_id })
        }
        (_, None) => match finished {
            Some(s) => {
                let exit = s.exit.clone().unwrap_or_default();
                // An update-mode session that finishes without an error is a success.
                if let (LinkMode::Update { item_id }, None) = (mode, &exit.error) {
                    if s.exit.is_none() {
                        vault.with_conn(|c| store::set_status(c, *item_id, store::STATUS_ACTIVE, None))?;
                        return Ok(LinkPoll::Reconnected { item_id: *item_id });
                    }
                }
                let message = exit
                    .error
                    .and_then(|e| e.display_message.or(Some(e.error_message)).filter(|m| !m.is_empty()))
                    .unwrap_or_else(|| "The connection was closed before it finished.".into());
                Ok(LinkPoll::Exited { message })
            }
            None => Ok(LinkPoll::Waiting),
        },
    }
}

pub fn item_accounts(api: &impl PlaidApi, vault: &impl VaultAccess, item_id: i64) -> Result<Vec<PlaidAccountView>> {
    let token = vault.with_conn(|c| Ok(store::item_secret(c, item_id)?.access_token))?;
    check_env(api, &token)?;
    let accounts = api.get_accounts(&token).map_err(|e| record_error(vault, item_id, e))?;
    vault.with_conn(|c| store::account_views(c, item_id, &accounts))
}

pub fn link_accounts(
    api: &impl PlaidApi,
    vault: &impl VaultAccess,
    item_id: i64,
    choices: &[AccountChoice],
) -> Result<(LinkSummary, SyncResult)> {
    let token = vault.with_conn(|c| Ok(store::item_secret(c, item_id)?.access_token))?;
    check_env(api, &token)?;
    let accounts = api.get_accounts(&token).map_err(|e| record_error(vault, item_id, e))?;
    let summary = vault.with_conn(|c| store::apply_account_choices(c, item_id, &accounts, choices))?;
    let sync = sync_item(api, vault, item_id)?;
    Ok((summary, sync))
}

/// Save the error on the item and convert it.
fn record_error(vault: &impl VaultAccess, item_id: i64, e: PlaidError) -> CoreError {
    let status = if e.needs_reauth() { store::STATUS_NEEDS_REAUTH } else { store::STATUS_ERROR };
    let msg = e.user_message();
    let _ = vault.with_conn(|c| store::set_status(c, item_id, status, Some(&msg)));
    e.into()
}

fn fetch_sync_pages(api: &impl PlaidApi, token: &str, start: Option<String>) -> std::result::Result<Vec<SyncPage>, PlaidError> {
    let mut restarts = 0;
    'restart: loop {
        let mut pages: Vec<SyncPage> = Vec::new();
        let mut cursor = start.clone();
        loop {
            match api.sync_transactions(token, cursor.as_deref()) {
                Ok(page) => {
                    cursor = Some(page.next_cursor.clone());
                    let more = page.has_more;
                    pages.push(page);
                    if !more || pages.len() >= MAX_SYNC_PAGES {
                        return Ok(pages);
                    }
                }
                // Plaid: restart the whole pagination loop from the first cursor.
                Err(e) if e.code() == "TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION" && restarts < 3 => {
                    restarts += 1;
                    continue 'restart;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

const IGNORABLE_INVESTMENT_ERRORS: &[&str] = &[
    "NO_INVESTMENT_ACCOUNTS",
    "PRODUCTS_NOT_SUPPORTED",
    "PRODUCT_NOT_READY",
    "ADDITIONAL_CONSENT_REQUIRED",
    "INVALID_PRODUCT",
    "PRODUCT_NOT_ENABLED",
];

pub fn sync_item(api: &impl PlaidApi, vault: &impl VaultAccess, item_id: i64) -> Result<SyncResult> {
    let secret = vault.with_conn(|c| store::item_secret(c, item_id))?;
    check_env(api, &secret.access_token)?;
    let token = secret.access_token.as_str();
    let day = today();
    let mut result = SyncResult {
        item_id,
        institution_name: secret.institution_name.clone(),
        ..Default::default()
    };

    let accounts = api.get_accounts(token).map_err(|e| record_error(vault, item_id, e))?;
    result.balances_updated = vault.with_conn(|c| store::apply_balances(c, item_id, &accounts, &day))?;

    if secret.products.iter().any(|p| p == "transactions") {
        let pages = fetch_sync_pages(api, token, secret.cursor.clone()).map_err(|e| record_error(vault, item_id, e))?;
        result.history_pending = pages
            .last()
            .and_then(|p| p.transactions_update_status.as_deref())
            .is_some_and(|s| s == "NOT_READY" || s == "INITIAL_UPDATE_COMPLETE")
            || pages.last().is_some_and(|p| p.has_more);
        result.transactions = vault.with_conn(|c| store::apply_sync(c, item_id, &pages))?;
    }

    if secret.products.iter().any(|p| p == "investments") {
        match api.get_holdings(token) {
            Ok(h) => {
                result.holdings_updated = vault.with_conn(|c| store::apply_holdings(c, item_id, &h, &day))?;
            }
            Err(e) if IGNORABLE_INVESTMENT_ERRORS.contains(&e.code()) => {}
            Err(e) => return Err(record_error(vault, item_id, e)),
        }
    }

    vault.with_conn(|c| store::mark_synced(c, item_id))?;
    Ok(result)
}

/// Sync every item. Errors are reported per item; one failing bank doesn't
/// stop the others. Items connected in another environment, items with no
/// tracked accounts, and items waiting for the user to sign in are skipped.
pub fn sync_all(api: &impl PlaidApi, vault: &impl VaultAccess) -> Result<Vec<std::result::Result<SyncResult, (i64, String)>>> {
    let items = vault.with_conn(|c| store::list_items(c))?;
    Ok(items
        .into_iter()
        .filter(|i| {
            i.environment == Some(api.environment())
                && i.linked_account_count > 0
                && i.status != store::STATUS_NEEDS_REAUTH
        })
        .map(|i| sync_item(api, vault, i.id).map_err(|e| (i.id, e.to_string())))
        .collect())
}

/// Disconnect: revoke the access token at Plaid, then forget it locally.
/// With `force`, the local record is removed even if Plaid can't be reached.
pub fn remove_item(api: &impl PlaidApi, vault: &impl VaultAccess, item_id: i64, force: bool) -> Result<()> {
    let token = vault.with_conn(|c| Ok(store::item_secret(c, item_id)?.access_token))?;
    let env_ok = Environment::from_access_token(&token) == Some(api.environment());
    if env_ok {
        match api.remove_item(&token) {
            Ok(()) => {}
            Err(e) if matches!(e.code(), "ITEM_NOT_FOUND" | "INVALID_ACCESS_TOKEN") => {}
            Err(e) if force => {
                let _ = e;
            }
            Err(e) => return Err(e.into()),
        }
    } else if !force {
        check_env(api, &token)?;
    }
    vault.with_conn(|c| store::delete_item(c, item_id))
}

/// Sandbox only: create a test connection without opening Link.
pub fn add_sandbox_item(api: &impl PlaidApi, vault: &impl VaultAccess, products: &[String]) -> Result<LinkPoll> {
    if api.environment() != Environment::Sandbox {
        return Err(CoreError::Invalid("Test banks are only available in Sandbox.".into()));
    }
    let public = api.sandbox_public_token(SANDBOX_INSTITUTION, products)?;
    let exchange = api.exchange_public_token(&public)?;
    let name = api
        .get_item(&exchange.access_token)
        .ok()
        .and_then(|i| i.institution_name)
        .unwrap_or_else(|| "First Platypus Bank (test)".into());
    let item_id = vault.with_conn(|c| {
        store::upsert_item(c, &exchange.item_id, &exchange.access_token, Some(SANDBOX_INSTITUTION), Some(&name), products)
    })?;
    Ok(LinkPoll::Connected { item_id, institution_name: name })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use serde_json::json;

    use super::*;
    use crate::accounts::{self, tests::input};
    use crate::plaid::api::*;
    use crate::plaid::store::LinkAction;
    use crate::transactions::{self, TransactionFilter};

    struct TestVault(RefCell<Connection>);

    impl VaultAccess for TestVault {
        fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
            f(&mut self.0.borrow_mut())
        }
    }

    fn vault() -> TestVault {
        TestVault(RefCell::new(crate::accounts::tests::conn()))
    }

    #[derive(Default)]
    struct Mock {
        env: Option<Environment>,
        accounts: RefCell<Vec<PlaidAccount>>,
        pages: RefCell<VecDeque<PResult<SyncPage>>>,
        cursors_seen: RefCell<Vec<Option<String>>>,
        link_info: RefCell<Option<LinkTokenInfo>>,
        holdings: RefCell<Option<PResult<Holdings>>>,
        accounts_error: RefCell<Option<PlaidError>>,
        removed: RefCell<Vec<String>>,
        exchanges: RefCell<usize>,
        last_link_request: RefCell<Option<LinkTokenRequest>>,
        link_url: Option<String>,
    }

    fn api_err(code: &str) -> PlaidError {
        PlaidError::Api {
            status: 400,
            body: ApiErrorBody {
                error_type: "ITEM_ERROR".into(),
                error_code: code.into(),
                error_message: format!("{code} happened"),
                display_message: None,
            },
        }
    }

    impl PlaidApi for Mock {
        fn environment(&self) -> Environment {
            self.env.unwrap_or(Environment::Sandbox)
        }
        fn check_credentials(&self) -> PResult<()> {
            Ok(())
        }
        fn create_link_token(&self, req: &LinkTokenRequest) -> PResult<LinkTokenCreated> {
            *self.last_link_request.borrow_mut() = Some(req.clone());
            Ok(LinkTokenCreated {
                link_token: "link-sandbox-abc".into(),
                expiration: None,
                hosted_link_url: Some(self.link_url.clone().unwrap_or_else(|| "https://hosted.plaid.com/link/abc".into())),
            })
        }
        fn get_link_token(&self, _t: &str) -> PResult<LinkTokenInfo> {
            Ok(self.link_info.borrow().clone().unwrap_or_default())
        }
        fn exchange_public_token(&self, _p: &str) -> PResult<Exchange> {
            *self.exchanges.borrow_mut() += 1;
            Ok(Exchange { access_token: "access-sandbox-xyz".into(), item_id: "item-1".into() })
        }
        fn get_item(&self, _a: &str) -> PResult<ItemMeta> {
            Ok(ItemMeta { item_id: "item-1".into(), institution_id: None, institution_name: Some("Platypus".into()) })
        }
        fn get_accounts(&self, _a: &str) -> PResult<Vec<PlaidAccount>> {
            if let Some(e) = self.accounts_error.borrow().clone() {
                return Err(e);
            }
            Ok(self.accounts.borrow().clone())
        }
        fn sync_transactions(&self, _a: &str, cursor: Option<&str>) -> PResult<SyncPage> {
            self.cursors_seen.borrow_mut().push(cursor.map(String::from));
            self.pages.borrow_mut().pop_front().unwrap_or_else(|| Ok(page(vec![], vec![], vec![], "c-end", false)))
        }
        fn get_holdings(&self, _a: &str) -> PResult<Holdings> {
            self.holdings.borrow_mut().take().unwrap_or_else(|| Err(api_err("NO_INVESTMENT_ACCOUNTS")))
        }
        fn remove_item(&self, a: &str) -> PResult<()> {
            self.removed.borrow_mut().push(a.into());
            Ok(())
        }
        fn sandbox_public_token(&self, _i: &str, _p: &[String]) -> PResult<String> {
            Ok("public-sandbox-1".into())
        }
    }

    fn acct(id: &str, name: &str, t: &str, sub: &str, mask: &str, current: f64) -> PlaidAccount {
        serde_json::from_value(json!({
            "account_id": id, "name": name, "official_name": null, "mask": mask,
            "type": t, "subtype": sub,
            "balances": {"available": null, "current": current, "limit": null, "iso_currency_code": "USD"}
        }))
        .unwrap()
    }

    fn txn(id: &str, account: &str, amount: f64, date: &str, name: &str, primary: &str, detailed: &str) -> PlaidTransaction {
        serde_json::from_value(json!({
            "transaction_id": id, "account_id": account, "amount": amount, "iso_currency_code": "USD",
            "date": date, "authorized_date": null, "name": name, "merchant_name": name,
            "pending": false, "pending_transaction_id": null,
            "personal_finance_category": {"primary": primary, "detailed": detailed}
        }))
        .unwrap()
    }

    fn page(added: Vec<PlaidTransaction>, modified: Vec<PlaidTransaction>, removed: Vec<&str>, cursor: &str, more: bool) -> SyncPage {
        SyncPage {
            added,
            modified,
            removed: removed
                .into_iter()
                .map(|id| RemovedTransaction { transaction_id: id.into(), account_id: None })
                .collect(),
            next_cursor: cursor.into(),
            has_more: more,
            transactions_update_status: Some("HISTORICAL_UPDATE_COMPLETE".into()),
        }
    }

    fn connected_mock() -> Mock {
        let m = Mock::default();
        *m.accounts.borrow_mut() = vec![
            acct("pa-chk", "Plaid Checking", "depository", "checking", "0000", 110.0),
            acct("pa-cc", "Plaid Credit Card", "credit", "credit card", "3333", 410.25),
            acct("pa-401", "Plaid 401k", "investment", "401k", "7777", 23631.98),
        ];
        m
    }

    fn connect(m: &Mock, v: &TestVault, products: &[&str]) -> i64 {
        *m.link_info.borrow_mut() = Some(
            serde_json::from_value(json!({"link_sessions": [{
                "link_session_id": "s", "finished_at": "2026-09-16T00:00:00Z",
                "results": {"item_add_results": [{"public_token": "public-sandbox-1",
                    "institution": {"name": "First Platypus Bank", "institution_id": "ins_109508"}}]},
                "exit": null
            }]}))
            .unwrap(),
        );
        let mode = LinkMode::New { products: products.iter().map(|s| s.to_string()).collect() };
        match poll_link(m, v, "link-sandbox-abc", &mode).unwrap() {
            LinkPoll::Connected { item_id, institution_name } => {
                assert_eq!(institution_name, "First Platypus Bank");
                item_id
            }
            other => panic!("{other:?}"),
        }
    }

    fn create_all(item: i64, m: &Mock, v: &TestVault) -> (LinkSummary, SyncResult) {
        let choices: Vec<AccountChoice> = m
            .accounts
            .borrow()
            .iter()
            .map(|a| AccountChoice {
                plaid_account_id: a.account_id.clone(),
                action: LinkAction::Create,
                account_id: None,
                kind: None,
                name: None,
                sync_from: None,
            })
            .collect();
        link_accounts(m, v, item, &choices).unwrap()
    }

    #[test]
    fn start_link_new_and_update() {
        let m = Mock::default();
        let v = vault();
        let s = start_link(&m, &v, &LinkMode::New { products: vec!["transactions".into()] }).unwrap();
        assert_eq!(s.url, "https://hosted.plaid.com/link/abc");
        let req = m.last_link_request.borrow().clone().unwrap();
        assert!(req.client_user_id.starts_with("local-"));
        assert_eq!(req.days_requested, 730);
        assert!(req.access_token.is_none());

        let item = connect(&m, &v, &["transactions"]);
        start_link(&m, &v, &LinkMode::Update { item_id: item }).unwrap();
        let req = m.last_link_request.borrow().clone().unwrap();
        assert_eq!(req.access_token.as_deref(), Some("access-sandbox-xyz"));
        assert!(req.products.is_empty());

        let bad = Mock { link_url: Some("https://evil.example/".into()), ..Default::default() };
        assert!(start_link(&bad, &v, &LinkMode::New { products: vec![] }).is_err());
    }

    #[test]
    fn poll_waiting_exited_and_update() {
        let m = Mock::default();
        let v = vault();
        let mode = LinkMode::New { products: vec!["transactions".into()] };
        assert_eq!(poll_link(&m, &v, "t", &mode).unwrap(), LinkPoll::Waiting);
        *m.link_info.borrow_mut() = Some(
            serde_json::from_value(json!({"link_sessions": [{"link_session_id": "s", "started_at": "x", "finished_at": null}]})).unwrap(),
        );
        assert_eq!(poll_link(&m, &v, "t", &mode).unwrap(), LinkPoll::Waiting);
        *m.link_info.borrow_mut() = Some(
            serde_json::from_value(json!({"link_sessions": [{"finished_at": "x", "results": {"item_add_results": []},
                "exit": {"error": null, "metadata": {"status": "requires_credentials"}}}]}))
            .unwrap(),
        );
        assert!(matches!(poll_link(&m, &v, "t", &mode).unwrap(), LinkPoll::Exited { .. }));
        assert_eq!(*m.exchanges.borrow(), 0);

        let item = connect(&m, &v, &["transactions"]);
        v.with_conn(|c| store::set_status(c, item, store::STATUS_NEEDS_REAUTH, Some("x"))).unwrap();
        *m.link_info.borrow_mut() = Some(
            serde_json::from_value(json!({"link_sessions": [{"finished_at": "x", "results": null, "exit": null}]})).unwrap(),
        );
        assert_eq!(
            poll_link(&m, &v, "t", &LinkMode::Update { item_id: item }).unwrap(),
            LinkPoll::Reconnected { item_id: item }
        );
        let items = v.with_conn(|c| store::list_items(c)).unwrap();
        assert_eq!(items[0].status, "active");
    }

    #[test]
    fn full_connect_and_sync_flow() {
        let m = connected_mock();
        let v = vault();
        let item = connect(&m, &v, &["transactions"]);
        let views = item_accounts(&m, &v, item).unwrap();
        assert_eq!(views.len(), 3);
        assert_eq!(views[1].suggested_kind, "credit_card");
        assert_eq!(views[2].suggested_kind, "401k");
        assert_eq!(views[1].balance_cents, Some(41025));

        m.pages.borrow_mut().extend([
            Ok(page(
                vec![
                    txn("t1", "pa-chk", 5.4, "2026-09-01", "Starbucks", "FOOD_AND_DRINK", "FOOD_AND_DRINK_COFFEE"),
                    txn("t2", "pa-chk", -2500.0, "2026-09-01", "Acme Payroll", "INCOME", "INCOME_SALARY"),
                    txn("t-other", "pa-unknown", 1.0, "2026-09-01", "Ghost", "OTHER", "OTHER"),
                ],
                vec![],
                vec![],
                "c1",
                true,
            )),
            Ok(page(
                vec![txn("t3", "pa-cc", 89.99, "2026-09-02", "Whole Foods", "FOOD_AND_DRINK", "FOOD_AND_DRINK_GROCERIES")],
                vec![],
                vec![],
                "c2",
                false,
            )),
        ]);
        let (summary, sync) = create_all(item, &m, &v);
        assert_eq!(summary.created, 3);
        assert_eq!(sync.transactions.added, 3);
        assert_eq!(sync.transactions.skipped, 1);
        assert_eq!(sync.balances_updated, 3);
        assert_eq!(*m.cursors_seen.borrow(), vec![None, Some("c1".to_string())]);

        v.with_conn(|c| {
            let accts = accounts::list(c, false)?;
            let cc = accts.iter().find(|a| a.kind == "credit_card").unwrap();
            assert!(!cc.is_asset && cc.linked);
            assert_eq!(cc.balance_cents, Some(41025));
            assert_eq!(cc.last4.as_deref(), Some("3333"));
            assert_eq!(cc.institution.as_deref(), Some("First Platypus Bank"));
            let page = transactions::query(c, &TransactionFilter::default())?;
            let wf = page.items.iter().find(|t| t.payee == "Whole Foods").unwrap();
            assert_eq!(wf.amount_cents, -8999);
            assert_eq!(wf.category_name.as_deref(), Some("Groceries"));
            assert_eq!(wf.source, "plaid");
            let pay = page.items.iter().find(|t| t.payee == "Acme Payroll").unwrap();
            assert_eq!(pay.amount_cents, 250000);
            assert_eq!(pay.category_name.as_deref(), Some("Income"));
            let nw = accounts::net_worth(c)?;
            assert_eq!(nw.net_cents, 11000 + 2363198 - 41025);
            Ok(())
        })
        .unwrap();

        // Second sync: continues from the saved cursor; modify keeps the
        // user's category; remove deletes.
        v.with_conn(|c| {
            let page = transactions::query(c, &TransactionFilter::default())?;
            let t1 = page.items.iter().find(|t| t.payee == "Starbucks").unwrap();
            let shopping = crate::categories::find_by_name(c, "Shopping")?.unwrap();
            transactions::set_category(c, &[t1.id], Some(shopping))?;
            Ok(())
        })
        .unwrap();
        let mut modified = txn("t1", "pa-chk", 6.0, "2026-09-01", "Starbucks Coffee", "FOOD_AND_DRINK", "FOOD_AND_DRINK_COFFEE");
        modified.pending = false;
        m.pages.borrow_mut().push_back(Ok(page(vec![], vec![modified], vec!["t3"], "c3", false)));
        let s2 = sync_item(&m, &v, item).unwrap();
        assert_eq!((s2.transactions.modified, s2.transactions.removed), (1, 1));
        assert_eq!(m.cursors_seen.borrow().last().unwrap().as_deref(), Some("c2"));
        v.with_conn(|c| {
            let page = transactions::query(c, &TransactionFilter::default())?;
            assert_eq!(page.total, 2);
            let t1 = page.items.iter().find(|t| t.payee == "Starbucks Coffee").unwrap();
            assert_eq!(t1.amount_cents, -600);
            assert_eq!(t1.category_name.as_deref(), Some("Shopping"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn pending_category_carries_over_and_learned_payees_win() {
        let m = connected_mock();
        let v = vault();
        let item = connect(&m, &v, &["transactions"]);
        let mut pending = txn("p1", "pa-chk", 20.0, "2026-09-03", "Corner Store", "GENERAL_MERCHANDISE", "X");
        pending.pending = true;
        m.pages.borrow_mut().push_back(Ok(page(vec![pending], vec![], vec![], "c1", false)));
        create_all(item, &m, &v);
        v.with_conn(|c| {
            let page = transactions::query(c, &TransactionFilter::default())?;
            let dining = crate::categories::find_by_name(c, "Dining")?.unwrap();
            transactions::set_category(c, &[page.items[0].id], Some(dining))?;
            Ok(())
        })
        .unwrap();
        let mut posted = txn("x1", "pa-chk", 21.5, "2026-09-04", "Corner Store", "GENERAL_MERCHANDISE", "X");
        posted.pending_transaction_id = Some("p1".into());
        let other = txn("x2", "pa-chk", 3.0, "2026-09-05", "CORNER STORE", "GENERAL_MERCHANDISE", "X");
        m.pages.borrow_mut().push_back(Ok(page(vec![posted, other], vec![], vec!["p1"], "c2", false)));
        sync_item(&m, &v, item).unwrap();
        v.with_conn(|c| {
            let page = transactions::query(c, &TransactionFilter::default())?;
            assert_eq!(page.total, 2);
            assert!(page.items.iter().all(|t| t.category_name.as_deref() == Some("Dining")));
            assert!(page.items.iter().all(|t| !t.pending));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn linking_existing_account_respects_sync_from() {
        let m = connected_mock();
        let v = vault();
        let existing = v
            .with_conn(|c| {
                let mut i = input("My Checking", "checking");
                i.last4 = Some("0000".into());
                let a = accounts::create(c, &i)?;
                transactions::create(c, &transactions::tests::tx(a.id, "2026-08-31", -100, "Old manual"))?;
                Ok(a.id)
            })
            .unwrap();
        let item = connect(&m, &v, &["transactions"]);
        let views = item_accounts(&m, &v, item).unwrap();
        assert_eq!(views[0].suggested_match_id, Some(existing));
        assert_eq!(views[0].suggested_sync_from.as_deref(), Some("2026-09-01"));
        assert_eq!(views[1].suggested_match_id, None);

        m.pages.borrow_mut().push_back(Ok(page(
            vec![
                txn("old", "pa-chk", 1.0, "2026-08-31", "Old manual", "OTHER", "OTHER"),
                txn("new", "pa-chk", 2.0, "2026-09-01", "New one", "OTHER", "OTHER"),
            ],
            vec![],
            vec![],
            "c1",
            false,
        )));
        let choices = vec![
            AccountChoice {
                plaid_account_id: "pa-chk".into(),
                action: LinkAction::Link,
                account_id: Some(existing),
                kind: None,
                name: None,
                sync_from: views[0].suggested_sync_from.clone(),
            },
            AccountChoice { plaid_account_id: "pa-cc".into(), action: LinkAction::Skip, account_id: None, kind: None, name: None, sync_from: None },
        ];
        let (summary, sync) = link_accounts(&m, &v, item, &choices).unwrap();
        assert_eq!((summary.linked, summary.created), (1, 0));
        assert_eq!(sync.transactions.added, 1);
        assert_eq!(sync.transactions.skipped, 1);
        v.with_conn(|c| {
            assert_eq!(accounts::list(c, false)?.len(), 1);
            assert_eq!(transactions::query(c, &TransactionFilter::default())?.total, 2);
            Ok(())
        })
        .unwrap();

        // Two Plaid accounts can't target the same local account.
        let dup = vec![
            AccountChoice { plaid_account_id: "pa-cc".into(), action: LinkAction::Link, account_id: Some(existing), kind: None, name: None, sync_from: None },
        ];
        assert!(link_accounts(&m, &v, item, &dup).is_err());
        // Unknown Plaid account ids are rejected.
        let bogus = vec![AccountChoice { plaid_account_id: "nope".into(), action: LinkAction::Create, account_id: None, kind: None, name: None, sync_from: None }];
        assert!(link_accounts(&m, &v, item, &bogus).is_err());

        // Linking another account later resets the cursor for a full re-sync,
        // without duplicating what's already there.
        let later = vec![AccountChoice { plaid_account_id: "pa-cc".into(), action: LinkAction::Create, account_id: None, kind: None, name: Some("Card".into()), sync_from: None }];
        m.pages.borrow_mut().push_back(Ok(page(
            vec![
                txn("new", "pa-chk", 2.0, "2026-09-01", "New one", "OTHER", "OTHER"),
                txn("cc1", "pa-cc", 9.0, "2026-09-02", "Card buy", "OTHER", "OTHER"),
            ],
            vec![],
            vec![],
            "c9",
            false,
        )));
        link_accounts(&m, &v, item, &later).unwrap();
        assert_eq!(m.cursors_seen.borrow().last().unwrap(), &None);
        v.with_conn(|c| {
            assert_eq!(transactions::query(c, &TransactionFilter::default())?.total, 3);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn pagination_restarts_on_mutation() {
        let m = connected_mock();
        let v = vault();
        let item = connect(&m, &v, &["transactions"]);
        m.pages.borrow_mut().extend([
            Ok(page(vec![txn("a", "pa-chk", 1.0, "2026-09-01", "A", "OTHER", "OTHER")], vec![], vec![], "c1", true)),
            Err(api_err("TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION")),
            Ok(page(vec![txn("a", "pa-chk", 1.0, "2026-09-01", "A", "OTHER", "OTHER")], vec![], vec![], "c1", true)),
            Ok(page(vec![txn("b", "pa-chk", 1.0, "2026-09-02", "B", "OTHER", "OTHER")], vec![], vec![], "c2", false)),
        ]);
        let (_, sync) = create_all(item, &m, &v);
        assert_eq!(sync.transactions.added, 2);
        assert_eq!(
            *m.cursors_seen.borrow(),
            vec![None, Some("c1".to_string()), None, Some("c1".to_string())]
        );
    }

    #[test]
    fn login_required_marks_item_and_other_errors_leave_data() {
        let m = connected_mock();
        let v = vault();
        let item = connect(&m, &v, &["transactions"]);
        create_all(item, &m, &v);
        *m.accounts_error.borrow_mut() = Some(api_err("ITEM_LOGIN_REQUIRED"));
        let err = sync_item(&m, &v, item).unwrap_err();
        assert!(matches!(&err, CoreError::Plaid { code, .. } if code == "ITEM_LOGIN_REQUIRED"));
        let items = v.with_conn(|c| store::list_items(c)).unwrap();
        assert_eq!(items[0].status, "needs_reauth");
        assert!(items[0].last_error.as_deref().unwrap().contains("Reconnect"));

        // A sync-page failure saves nothing and keeps the old cursor.
        *m.accounts_error.borrow_mut() = None;
        m.pages.borrow_mut().extend([
            Ok(page(vec![txn("z", "pa-chk", 1.0, "2026-09-01", "Z", "OTHER", "OTHER")], vec![], vec![], "cz", true)),
            Err(api_err("INTERNAL_SERVER_ERROR")),
        ]);
        assert!(sync_item(&m, &v, item).is_err());
        v.with_conn(|c| {
            assert_eq!(transactions::query(c, &TransactionFilter::default())?.total, 0);
            Ok(())
        })
        .unwrap();
        let items = v.with_conn(|c| store::list_items(c)).unwrap();
        assert_eq!(items[0].status, "error");
        // Success clears the error.
        sync_item(&m, &v, item).unwrap();
        let items = v.with_conn(|c| store::list_items(c)).unwrap();
        assert_eq!((items[0].status.as_str(), items[0].last_error.as_deref()), ("active", None));
    }

    #[test]
    fn holdings_for_investment_items() {
        let m = connected_mock();
        let v = vault();
        let item = connect(&m, &v, &["investments"]);
        *m.holdings.borrow_mut() = Some(Ok(serde_json::from_value(json!({
            "accounts": [{"account_id": "pa-401", "name": "401k", "type": "investment", "subtype": "401k", "balances": {"current": 23631.98}}],
            "holdings": [
                {"account_id": "pa-401", "security_id": "s1", "institution_price": 100.0, "institution_value": 1500.5, "cost_basis": 1200.0, "quantity": 15.005},
                {"account_id": "pa-401", "security_id": "s2", "institution_price": 1.0, "institution_value": null, "cost_basis": null, "quantity": 20.0},
                {"account_id": "pa-unlinked", "security_id": "s1", "institution_price": 1.0, "institution_value": 1.0, "cost_basis": null, "quantity": 1.0}
            ],
            "securities": [
                {"security_id": "s1", "name": "Vanguard Total Stock Market", "ticker_symbol": "VTI", "type": "etf"},
                {"security_id": "s2", "name": null, "ticker_symbol": "CASH", "type": "cash"}
            ]
        }))
        .unwrap()));
        let (_, sync) = create_all(item, &m, &v);
        assert_eq!(sync.holdings_updated, 2);
        assert!(m.cursors_seen.borrow().is_empty(), "investments-only items don't call transactions/sync");
        let holdings = v
            .with_conn(|c| {
                let a = accounts::list(c, false)?.into_iter().find(|a| a.kind == "401k").unwrap();
                store::list_holdings(c, a.id)
            })
            .unwrap();
        assert_eq!(holdings.len(), 2);
        assert_eq!(holdings[0].ticker.as_deref(), Some("VTI"));
        assert_eq!(holdings[0].value_cents, 150050);
        assert_eq!(holdings[1].value_cents, 2000);
        assert_eq!(holdings[1].name, "CASH");

        // A later sync without investment data is fine.
        sync_item(&m, &v, item).unwrap();
    }

    #[test]
    fn environment_mismatch_and_remove() {
        let m = connected_mock();
        let v = vault();
        let item = connect(&m, &v, &["transactions"]);
        create_all(item, &m, &v);
        let prod = Mock { env: Some(Environment::Production), ..connected_mock() };
        assert!(sync_item(&prod, &v, item).unwrap_err().to_string().contains("sandbox"));
        let results = sync_all(&prod, &v).unwrap();
        assert!(results.is_empty(), "items from another environment are skipped");
        assert!(remove_item(&prod, &v, item, false).is_err());

        remove_item(&m, &v, item, false).unwrap();
        assert_eq!(*m.removed.borrow(), vec!["access-sandbox-xyz".to_string()]);
        v.with_conn(|c| {
            assert!(store::list_items(c)?.is_empty());
            let accts = accounts::list(c, false)?;
            assert_eq!(accts.len(), 3, "accounts and history are kept");
            assert!(accts.iter().all(|a| !a.linked));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn sandbox_test_item() {
        let m = connected_mock();
        let v = vault();
        let r = add_sandbox_item(&m, &v, &["transactions".into()]).unwrap();
        assert!(matches!(r, LinkPoll::Connected { .. }));
        let prod = Mock { env: Some(Environment::Production), ..Default::default() };
        assert!(add_sandbox_item(&prod, &v, &["transactions".into()]).is_err());
        assert_eq!(products_for("banking").unwrap(), vec!["transactions".to_string()]);
        assert!(products_for("crypto").is_err());
    }

    #[test]
    fn link_poll_json_shape() {
        let j = serde_json::to_value(LinkPoll::Connected { item_id: 3, institution_name: "X".into() }).unwrap();
        assert_eq!(j, json!({"state": "connected", "itemId": 3, "institutionName": "X"}));
        let j = serde_json::to_value(LinkPoll::Waiting).unwrap();
        assert_eq!(j, json!({"state": "waiting"}));
    }
}
