//! Live bank connections through Plaid.
//!
//! * `api`     - HTTP client and response types
//! * `config`  - credentials stored in the encrypted vault
//! * `mapping` - Plaid types/categories to this app's
//! * `store`   - database side (no network)
//! * `service` - flows that combine network calls and database updates,
//!   written so the vault is never held locked-open during a network call

pub mod api;
pub mod config;
pub mod mapping;
pub mod service;
pub mod store;

pub use api::{Environment, HttpPlaid, PlaidApi};
