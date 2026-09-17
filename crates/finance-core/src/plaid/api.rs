//! Plaid API client.
//!
//! Only the endpoints this app needs, with typed requests and responses.
//! Security properties of the HTTP client:
//! * HTTPS only, to a fixed Plaid host chosen by environment
//! * no redirects followed, 30 s timeout, responses capped at 20 MB
//! * credentials are sent in the JSON body (Plaid's scheme) and never logged

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::error::CoreError;

pub const PLAID_VERSION: &str = "2020-09-14";
const MAX_RESPONSE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Sandbox,
    Production,
}

impl Environment {
    pub fn base_url(self) -> &'static str {
        match self {
            Environment::Sandbox => "https://sandbox.plaid.com",
            Environment::Production => "https://production.plaid.com",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Sandbox => "sandbox",
            Environment::Production => "production",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sandbox" => Some(Environment::Sandbox),
            "production" => Some(Environment::Production),
            _ => None,
        }
    }

    /// Plaid access tokens are prefixed with their environment.
    pub fn from_access_token(token: &str) -> Option<Self> {
        if token.starts_with("access-sandbox-") {
            Some(Environment::Sandbox)
        } else if token.starts_with("access-production-") {
            Some(Environment::Production)
        } else {
            None
        }
    }
}

// ------------------------------------------------------------------ errors

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ApiErrorBody {
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub error_message: String,
    pub display_message: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PlaidError {
    /// Plaid answered with an error object.
    Api { status: u16, body: ApiErrorBody },
    /// Could not reach Plaid or the connection failed.
    Network(String),
    /// Plaid's response could not be understood.
    Decode(String),
}

impl PlaidError {
    pub fn code(&self) -> &str {
        match self {
            PlaidError::Api { body, .. } => &body.error_code,
            PlaidError::Network(_) => "NETWORK_ERROR",
            PlaidError::Decode(_) => "DECODE_ERROR",
        }
    }

    /// The Item needs the user to sign in again (update mode).
    pub fn needs_reauth(&self) -> bool {
        matches!(
            self.code(),
            "ITEM_LOGIN_REQUIRED"
                | "PENDING_EXPIRATION"
                | "PENDING_DISCONNECT"
                | "ACCESS_NOT_GRANTED"
                | "NO_ACCOUNTS"
                | "INSUFFICIENT_CREDENTIALS"
                | "USER_PERMISSION_REVOKED"
                | "ITEM_LOCKED"
        )
    }

    pub fn user_message(&self) -> String {
        match self {
            PlaidError::Api { body, .. } => {
                let detail = body
                    .display_message
                    .clone()
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| body.error_message.clone());
                match body.error_code.as_str() {
                    "INVALID_API_KEYS" => {
                        "Plaid rejected the client ID or secret. Check them in Settings and make sure the environment matches the keys.".into()
                    }
                    "ITEM_LOGIN_REQUIRED" => {
                        "This bank needs you to sign in again. Use Reconnect.".into()
                    }
                    "INSTITUTION_DOWN" | "INSTITUTION_NOT_RESPONDING" => {
                        "The bank isn't responding right now. Try again later.".into()
                    }
                    "RATE_LIMIT_EXCEEDED" => "Plaid is rate limiting requests. Wait a minute and try again.".into(),
                    _ if detail.is_empty() => format!("Plaid error {}", body.error_code),
                    _ => format!("{detail} ({})", body.error_code),
                }
            }
            PlaidError::Network(e) => format!("Couldn't reach Plaid: {e}"),
            PlaidError::Decode(e) => format!("Unexpected response from Plaid: {e}"),
        }
    }
}

impl From<PlaidError> for CoreError {
    fn from(e: PlaidError) -> Self {
        CoreError::Plaid { code: e.code().to_string(), message: e.user_message() }
    }
}

pub type PResult<T> = std::result::Result<T, PlaidError>;

// ------------------------------------------------------------------- types

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Balances {
    pub available: Option<f64>,
    pub current: Option<f64>,
    pub limit: Option<f64>,
    pub iso_currency_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PlaidAccount {
    pub account_id: String,
    pub name: String,
    pub official_name: Option<String>,
    pub mask: Option<String>,
    #[serde(rename = "type")]
    pub account_type: String,
    pub subtype: Option<String>,
    #[serde(default)]
    pub balances: Balances,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PersonalFinanceCategory {
    pub primary: String,
    pub detailed: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PlaidTransaction {
    pub transaction_id: String,
    pub account_id: String,
    /// Positive = money out of the account (Plaid's convention).
    pub amount: f64,
    pub iso_currency_code: Option<String>,
    pub date: String,
    pub authorized_date: Option<String>,
    pub name: Option<String>,
    pub merchant_name: Option<String>,
    #[serde(default)]
    pub pending: bool,
    pub pending_transaction_id: Option<String>,
    pub personal_finance_category: Option<PersonalFinanceCategory>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RemovedTransaction {
    pub transaction_id: String,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SyncPage {
    #[serde(default)]
    pub added: Vec<PlaidTransaction>,
    #[serde(default)]
    pub modified: Vec<PlaidTransaction>,
    #[serde(default)]
    pub removed: Vec<RemovedTransaction>,
    pub next_cursor: String,
    #[serde(default)]
    pub has_more: bool,
    pub transactions_update_status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Holding {
    pub account_id: String,
    pub security_id: String,
    pub institution_price: Option<f64>,
    pub institution_value: Option<f64>,
    pub cost_basis: Option<f64>,
    pub quantity: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Security {
    pub security_id: String,
    pub name: Option<String>,
    pub ticker_symbol: Option<String>,
    #[serde(rename = "type")]
    pub security_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Holdings {
    #[serde(default)]
    pub accounts: Vec<PlaidAccount>,
    #[serde(default)]
    pub holdings: Vec<Holding>,
    #[serde(default)]
    pub securities: Vec<Security>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LinkTokenCreated {
    pub link_token: String,
    pub expiration: Option<String>,
    pub hosted_link_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Institution {
    pub name: Option<String>,
    pub institution_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ItemAddResult {
    pub public_token: String,
    pub institution: Option<Institution>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct LinkResults {
    #[serde(default)]
    pub item_add_results: Vec<ItemAddResult>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ExitMetadata {
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LinkExit {
    pub error: Option<ApiErrorBody>,
    pub metadata: Option<ExitMetadata>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LinkSession {
    pub link_session_id: Option<String>,
    pub finished_at: Option<String>,
    pub results: Option<LinkResults>,
    pub exit: Option<LinkExit>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LinkTokenInfo {
    #[serde(default)]
    pub link_sessions: Vec<LinkSession>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Exchange {
    pub access_token: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ItemMeta {
    pub item_id: String,
    pub institution_id: Option<String>,
    pub institution_name: Option<String>,
}

/// What a new Link session asks for.
#[derive(Debug, Clone)]
pub struct LinkTokenRequest {
    pub client_user_id: String,
    /// Empty in update mode.
    pub products: Vec<String>,
    /// Set for update mode (re-authenticate an existing Item).
    pub access_token: Option<String>,
    pub days_requested: u32,
}

// ------------------------------------------------------------------- trait

/// The Plaid calls the app uses. Implemented by [`HttpPlaid`] and by a mock
/// in tests.
pub trait PlaidApi {
    fn environment(&self) -> Environment;
    fn check_credentials(&self) -> PResult<()>;
    fn create_link_token(&self, req: &LinkTokenRequest) -> PResult<LinkTokenCreated>;
    fn get_link_token(&self, link_token: &str) -> PResult<LinkTokenInfo>;
    fn exchange_public_token(&self, public_token: &str) -> PResult<Exchange>;
    fn get_item(&self, access_token: &str) -> PResult<ItemMeta>;
    fn get_accounts(&self, access_token: &str) -> PResult<Vec<PlaidAccount>>;
    fn sync_transactions(&self, access_token: &str, cursor: Option<&str>) -> PResult<SyncPage>;
    fn get_holdings(&self, access_token: &str) -> PResult<Holdings>;
    fn remove_item(&self, access_token: &str) -> PResult<()>;
    fn sandbox_public_token(&self, institution_id: &str, products: &[String]) -> PResult<String>;
}

// -------------------------------------------------------------------- http

pub struct HttpPlaid {
    env: Environment,
    base: String,
    client_id: String,
    secret: Zeroizing<String>,
    agent: ureq::Agent,
}

impl HttpPlaid {
    pub fn new(env: Environment, client_id: &str, secret: &str) -> Self {
        Self::build(env, env.base_url().to_string(), true, client_id, secret)
    }

    fn build(env: Environment, base: String, https_only: bool, client_id: &str, secret: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(https_only)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .user_agent("finance-app/0.1")
            .build();
        HttpPlaid {
            env,
            base,
            client_id: client_id.to_string(),
            secret: Zeroizing::new(secret.to_string()),
            agent: ureq::Agent::new_with_config(config),
        }
    }

    fn post<T: DeserializeOwned>(&self, path: &str, mut body: Value) -> PResult<T> {
        if let Value::Object(map) = &mut body {
            map.insert("client_id".into(), Value::String(self.client_id.clone()));
            map.insert("secret".into(), Value::String(self.secret.to_string()));
        }
        let url = format!("{}{}", self.base, path);
        let mut resp = self
            .agent
            .post(&url)
            .header("Plaid-Version", PLAID_VERSION)
            .send_json(&body)
            .map_err(|e| PlaidError::Network(e.to_string()))?;
        // Drop our copy of the secret-bearing body promptly.
        drop(body);
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(|e| PlaidError::Network(e.to_string()))?;
        if status == 200 {
            serde_json::from_str(&text).map_err(|e| PlaidError::Decode(e.to_string()))
        } else {
            let body: ApiErrorBody = serde_json::from_str(&text).unwrap_or_else(|_| ApiErrorBody {
                error_type: "HTTP_ERROR".into(),
                error_code: format!("HTTP_{status}"),
                error_message: format!("Plaid returned HTTP {status}"),
                display_message: None,
            });
            Err(PlaidError::Api { status, body })
        }
    }
}

#[derive(Deserialize)]
struct AccountsResponse {
    accounts: Vec<PlaidAccount>,
}

#[derive(Deserialize)]
struct ItemResponse {
    item: ItemMeta,
}

#[derive(Deserialize)]
struct PublicTokenResponse {
    public_token: String,
}

#[derive(Deserialize)]
struct Empty {}

impl PlaidApi for HttpPlaid {
    fn environment(&self) -> Environment {
        self.env
    }

    fn check_credentials(&self) -> PResult<()> {
        let _: Empty =
            self.post("/institutions/get", json!({"count": 1, "offset": 0, "country_codes": ["US"]}))?;
        Ok(())
    }

    fn create_link_token(&self, req: &LinkTokenRequest) -> PResult<LinkTokenCreated> {
        let mut body = json!({
            "client_name": "Finance",
            "language": "en",
            "country_codes": ["US"],
            "user": { "client_user_id": req.client_user_id },
            "hosted_link": {},
        });
        if let Some(token) = &req.access_token {
            body["access_token"] = json!(token);
        } else {
            body["products"] = json!(req.products);
            if req.products.iter().any(|p| p == "transactions") {
                body["transactions"] = json!({ "days_requested": req.days_requested });
            }
        }
        self.post("/link/token/create", body)
    }

    fn get_link_token(&self, link_token: &str) -> PResult<LinkTokenInfo> {
        self.post("/link/token/get", json!({ "link_token": link_token }))
    }

    fn exchange_public_token(&self, public_token: &str) -> PResult<Exchange> {
        self.post("/item/public_token/exchange", json!({ "public_token": public_token }))
    }

    fn get_item(&self, access_token: &str) -> PResult<ItemMeta> {
        let r: ItemResponse = self.post("/item/get", json!({ "access_token": access_token }))?;
        Ok(r.item)
    }

    fn get_accounts(&self, access_token: &str) -> PResult<Vec<PlaidAccount>> {
        let r: AccountsResponse = self.post("/accounts/get", json!({ "access_token": access_token }))?;
        Ok(r.accounts)
    }

    fn sync_transactions(&self, access_token: &str, cursor: Option<&str>) -> PResult<SyncPage> {
        let mut body = json!({ "access_token": access_token, "count": 500 });
        if let Some(c) = cursor.filter(|c| !c.is_empty()) {
            body["cursor"] = json!(c);
        }
        self.post("/transactions/sync", body)
    }

    fn get_holdings(&self, access_token: &str) -> PResult<Holdings> {
        self.post("/investments/holdings/get", json!({ "access_token": access_token }))
    }

    fn remove_item(&self, access_token: &str) -> PResult<()> {
        let _: Empty = self.post("/item/remove", json!({ "access_token": access_token }))?;
        Ok(())
    }

    fn sandbox_public_token(&self, institution_id: &str, products: &[String]) -> PResult<String> {
        if self.env != Environment::Sandbox {
            return Err(PlaidError::Decode("Test banks are only available in Sandbox.".into()));
        }
        let r: PublicTokenResponse = self.post(
            "/sandbox/public_token/create",
            json!({ "institution_id": institution_id, "initial_products": products }),
        )?;
        Ok(r.public_token)
    }
}

/// Accept only HTTPS URLs on a plaid.com host before opening a browser.
pub fn validate_link_url(url: &str) -> Result<String, CoreError> {
    let bad = || CoreError::Invalid("Plaid returned an unexpected link.".into());
    let uri: ureq::http::Uri = url.parse().map_err(|_| bad())?;
    if uri.scheme_str() != Some("https") {
        return Err(bad());
    }
    if uri.authority().map(|a| a.as_str().contains('@')).unwrap_or(true) {
        return Err(bad());
    }
    let host = uri.host().ok_or_else(bad)?.to_ascii_lowercase();
    if host != "plaid.com" && !host.ends_with(".plaid.com") {
        return Err(bad());
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// Minimal one-request-per-connection HTTP server for client tests.
    fn serve(responses: Vec<(u16, &'static str)>) -> (String, mpsc::Receiver<(String, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    head.push_str(&line);
                }
                let mut buf = vec![0; len];
                reader.read_exact(&mut buf).unwrap();
                tx.send((head, String::from_utf8(buf).unwrap())).unwrap();
                let reason = if status == 200 { "OK" } else { "Bad Request" };
                write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        (format!("http://{addr}"), rx)
    }

    #[test]
    fn http_client_sends_credentials_and_parses_responses() {
        let (base, rx) = serve(vec![
            (200, r#"{"accounts":[{"account_id":"a1","name":"Checking","official_name":null,"mask":"0000","type":"depository","subtype":"checking","balances":{"available":100,"current":110,"limit":null,"iso_currency_code":"USD"}}],"item":{},"request_id":"r1"}"#),
            (400, r#"{"error_type":"ITEM_ERROR","error_code":"ITEM_LOGIN_REQUIRED","error_message":"login changed","display_message":"Please sign in again","request_id":"r2"}"#),
            (500, "not json"),
            (200, "{not valid json"),
        ]);
        let api = HttpPlaid::build(Environment::Sandbox, base, false, "client123", "secret456");

        let accts = api.get_accounts("access-sandbox-1").unwrap();
        assert_eq!(accts[0].balances.current, Some(110.0));
        let (head, body) = rx.recv().unwrap();
        assert!(head.starts_with("POST /accounts/get "));
        assert!(head.to_ascii_lowercase().contains("plaid-version: 2020-09-14"));
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["client_id"], "client123");
        assert_eq!(body["secret"], "secret456");
        assert_eq!(body["access_token"], "access-sandbox-1");

        let e = api.get_accounts("access-sandbox-1").unwrap_err();
        assert_eq!(e.code(), "ITEM_LOGIN_REQUIRED");
        assert!(e.needs_reauth());
        let e = api.get_accounts("x").unwrap_err();
        assert_eq!(e.code(), "HTTP_500");
        let e = api.get_accounts("x").unwrap_err();
        assert!(matches!(e, PlaidError::Decode(_)));
    }

    #[test]
    fn link_token_request_shapes() {
        let (base, rx) = serve(vec![
            (200, r#"{"link_token":"link-sandbox-1","expiration":"2026-09-17T00:00:00Z","hosted_link_url":"https://hosted.plaid.com/link/x","request_id":"r"}"#),
            (200, r#"{"link_token":"link-sandbox-2","expiration":null,"hosted_link_url":"https://hosted.plaid.com/link/y","request_id":"r"}"#),
        ]);
        let api = HttpPlaid::build(Environment::Sandbox, base, false, "c", "s");
        let created = api
            .create_link_token(&LinkTokenRequest {
                client_user_id: "local-1".into(),
                products: vec!["transactions".into()],
                access_token: None,
                days_requested: 730,
            })
            .unwrap();
        assert_eq!(created.link_token, "link-sandbox-1");
        let (_, body) = rx.recv().unwrap();
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["products"], json!(["transactions"]));
        assert_eq!(body["transactions"]["days_requested"], 730);
        assert_eq!(body["hosted_link"], json!({}));
        assert_eq!(body["user"]["client_user_id"], "local-1");
        assert_eq!(body["country_codes"], json!(["US"]));
        assert!(body.get("access_token").is_none());

        api.create_link_token(&LinkTokenRequest {
            client_user_id: "local-1".into(),
            products: vec![],
            access_token: Some("access-sandbox-9".into()),
            days_requested: 730,
        })
        .unwrap();
        let (_, body) = rx.recv().unwrap();
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["access_token"], "access-sandbox-9");
        assert!(body.get("products").is_none(), "update mode must not send products");
    }

    #[test]
    fn production_client_refuses_plain_http() {
        let api = HttpPlaid::build(Environment::Production, "http://127.0.0.1:9".into(), true, "c", "s");
        assert!(matches!(api.get_accounts("x"), Err(PlaidError::Network(_))));
        assert_eq!(HttpPlaid::new(Environment::Production, "c", "s").base, "https://production.plaid.com");
    }

    #[test]
    fn link_url_validation() {
        assert!(validate_link_url("https://hosted.plaid.com/link/abc").is_ok());
        assert!(validate_link_url("http://hosted.plaid.com/link/abc").is_err());
        assert!(validate_link_url("https://plaid.com.evil.example/x").is_err());
        assert!(validate_link_url("https://evil.example/?plaid.com").is_err());
        assert!(validate_link_url("https://user@evil.example@hosted.plaid.com/").is_err());
        assert!(validate_link_url("file:///etc/passwd").is_err());
        assert!(validate_link_url("not a url").is_err());
    }

    #[test]
    fn token_environment() {
        assert_eq!(Environment::from_access_token("access-sandbox-123"), Some(Environment::Sandbox));
        assert_eq!(Environment::from_access_token("access-production-1"), Some(Environment::Production));
        assert_eq!(Environment::from_access_token("access-development-1"), None);
    }

    #[test]
    fn parses_sync_and_link_payloads() {
        let page: SyncPage = serde_json::from_value(json!({
            "added": [{
                "transaction_id": "t1", "account_id": "a1", "amount": 12.34,
                "iso_currency_code": "USD", "date": "2026-09-01", "authorized_date": null,
                "name": "STARBUCKS", "merchant_name": "Starbucks", "pending": false,
                "pending_transaction_id": null,
                "personal_finance_category": {"primary": "FOOD_AND_DRINK", "detailed": "FOOD_AND_DRINK_COFFEE", "confidence_level": "HIGH", "version": "v2"},
                "payment_channel": "in store", "location": {}
            }],
            "modified": [], "removed": [{"transaction_id": "t0", "account_id": "a1"}],
            "next_cursor": "c1", "has_more": false, "transactions_update_status": "HISTORICAL_UPDATE_COMPLETE",
            "accounts": [], "request_id": "r"
        }))
        .unwrap();
        assert_eq!(page.added[0].amount, 12.34);
        assert_eq!(page.removed[0].transaction_id, "t0");

        let info: LinkTokenInfo = serde_json::from_value(json!({
            "link_token": "link-sandbox-1", "created_at": "x", "expiration": "y",
            "link_sessions": [{
                "link_session_id": "s", "started_at": "x", "finished_at": "y",
                "results": {"item_add_results": [{"public_token": "public-sandbox-1",
                    "accounts": [{"id": "a", "name": "Checking", "mask": "0000", "type": "depository", "subtype": "checking"}],
                    "institution": {"name": "First Platypus Bank", "institution_id": "ins_109508"}}]},
                "exit": null, "events": []
            }]
        }))
        .unwrap();
        let r = &info.link_sessions[0].results.as_ref().unwrap().item_add_results[0];
        assert_eq!(r.public_token, "public-sandbox-1");

        let err: ApiErrorBody = serde_json::from_value(json!({
            "error_type": "ITEM_ERROR", "error_code": "ITEM_LOGIN_REQUIRED",
            "error_message": "the login details of this item have changed", "display_message": null, "request_id": "r"
        }))
        .unwrap();
        let e = PlaidError::Api { status: 400, body: err };
        assert!(e.needs_reauth());
        assert!(e.user_message().contains("Reconnect"));
    }
}
