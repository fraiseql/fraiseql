//! Unified error types for FraiseQL runtime crates.
//!
//! All runtime crates depend on this crate for error handling.
//!
//! # Canonical taxonomy
//!
//! [`FraiseQLError`] is the single root error type for the FraiseQL workspace.
//! It is built around two layers:
//!
//! 1. **Engine variants** (`Parse`, `Validation`, `Database`, `RateLimited`,
//!    `NotFound`, `ServiceUnavailable`, `Internal`, …) which the core and
//!    runtime crates raise directly.
//! 2. **Domain composition variants** (`Auth`, `Webhook`, `Observer`, `File`)
//!    which wrap subsystem error types via `From` impls owned by each
//!    subsystem crate (sqlx pattern). This lets `fraiseql-error` stay a leaf
//!    crate while still exposing one unified taxonomy.
//!
//! With the `axum-compat` feature enabled, [`FraiseQLError`] also implements
//! [`axum::response::IntoResponse`] so handlers can return
//! `Result<_, FraiseQLError>` directly; the conversion produces an
//! [`ErrorResponse`] JSON body with the appropriate HTTP status code.
//!
//! ```text
//! FraiseQLError
//!     ↓  IntoResponse (via fraiseql-error::http, feature `axum-compat`)
//! ErrorResponse { error, error_description, error_code, error_uri, details, retry_after }
//!     ↓  Json(response) + StatusCode
//! HTTP response body (application/json)
//! ```
//!
//! ## Security note
//!
//! All variants that might leak internal details (database messages, config values,
//! provider endpoints) return **generic** descriptions in the HTTP response body.
//! Raw error details are available only in structured server logs.

#![warn(missing_docs)]

mod config;
pub mod core_error;
mod file;
pub mod graphql_error;
#[cfg(feature = "axum-compat")]
mod http;

pub use config::ConfigError;
pub use core_error::{ErrorContext, FraiseQLError, Result, ValidationFieldError};
pub use file::FileError;
pub use graphql_error::{GraphQLError, GraphQLErrorLocation};
// Re-export for convenience — only available with the `axum-compat` feature
#[cfg(feature = "axum-compat")]
pub use http::{ErrorResponse, IntoHttpResponse};
