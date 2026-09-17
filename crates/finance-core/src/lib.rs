//! Core logic for the personal finance app.
//!
//! Everything that touches secrets or the database lives here, independent of
//! the Tauri shell, so it can be unit-tested without a window.

pub mod crypto;
pub mod db;
pub mod error;
pub mod money;
pub mod password;
pub mod recovery;
pub mod settings;
pub mod throttle;
pub mod vault;

pub use error::{CoreError, Result};
pub use vault::{Session, Vault};
