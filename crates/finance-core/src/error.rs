use serde::Serialize;

pub type Result<T> = std::result::Result<T, CoreError>;

/// Errors surfaced by the core. Messages are safe to show to the user: they
/// never include key material, passwords, or database contents.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("No vault exists yet.")]
    VaultMissing,
    #[error("A vault already exists.")]
    VaultExists,
    #[error("The vault is locked.")]
    Locked,
    #[error("Enter your master password to continue.")]
    PasswordRequired,
    #[error("Incorrect password.")]
    WrongPassword,
    #[error("That recovery key is not valid.")]
    InvalidRecoveryKey,
    #[error("{0}")]
    WeakPassword(String),
    #[error("Too many attempts. Try again in {retry_after_secs} seconds.")]
    TooManyAttempts { retry_after_secs: u64 },
    #[error("The vault file is damaged or was written by a newer version of the app.")]
    CorruptVault,
    #[error("{0}")]
    Invalid(String),
    #[error("Encryption error.")]
    Crypto,
    /// An error reported by Plaid or while talking to Plaid.
    #[error("{message}")]
    Plaid { code: String, message: String },
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Data format error: {0}")]
    Json(#[from] serde_json::Error),
}

impl CoreError {
    /// Stable machine-readable code for the UI.
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::VaultMissing => "vault_missing",
            CoreError::VaultExists => "vault_exists",
            CoreError::Locked => "locked",
            CoreError::PasswordRequired => "password_required",
            CoreError::WrongPassword => "wrong_password",
            CoreError::InvalidRecoveryKey => "invalid_recovery_key",
            CoreError::WeakPassword(_) => "weak_password",
            CoreError::TooManyAttempts { .. } => "too_many_attempts",
            CoreError::CorruptVault => "corrupt_vault",
            CoreError::Invalid(_) => "invalid",
            CoreError::Crypto => "crypto",
            CoreError::Plaid { .. } => "plaid",
            CoreError::Db(_) => "db",
            CoreError::Io(_) => "io",
            CoreError::Json(_) => "json",
        }
    }
}

/// Shape sent to the frontend.
#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    /// Plaid's error_code (e.g. ITEM_LOGIN_REQUIRED) when code == "plaid".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plaid_code: Option<String>,
}

impl From<&CoreError> for ErrorPayload {
    fn from(e: &CoreError) -> Self {
        let retry_after_secs = match e {
            CoreError::TooManyAttempts { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        };
        let plaid_code = match e {
            CoreError::Plaid { code, .. } => Some(code.clone()),
            _ => None,
        };
        ErrorPayload { code: e.code(), message: e.to_string(), retry_after_secs, plaid_code }
    }
}
