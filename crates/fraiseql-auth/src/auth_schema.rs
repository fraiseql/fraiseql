//! Every DDL script first-party auth applies, in an order that satisfies its foreign keys.
//!
//! The server provisions these per feature (`fraiseql-server`'s `provision.rs`), so no
//! single deployment runs all of them. This list exists so that one test can: the
//! inventory suite applies it to an empty database and asserts which tables exist and
//! where. A script added to the crate and left off this list is a table that test cannot
//! see, so add it here in the same change.

use crate::{
    EMAIL_VERIFICATION_SCHEMA_SQL, PASSWORD_RESET_SCHEMA_SQL, PASSWORD_SCHEMA_SQL,
    PG_MFA_SCHEMA_SQL, PG_OTP_SCHEMA_SQL, account_linking, scim::PG_SCIM_SCHEMA_SQL,
    session_postgres::SESSIONS_SCHEMA_SQL,
};

/// What every auth script writes on schema `auth`, and the comment it refuses to find
/// missing on a schema `auth` that already exists.
pub const MARKER: &str = "fraiseql: first-party auth";

/// The head every auth script starts with: refuse a schema `auth` that FraiseQL did not
/// create, then create it if absent and mark it with [`MARKER`].
///
/// A database that has a schema `auth` of its own would otherwise receive auth's tables
/// beside the application's, or keep an application table of the same name that auth's
/// queries then read.
pub const CLAIM_SQL: &str = claim_auth_schema!();

/// The auth DDL that is always compiled in. The account store comes first: the
/// credential, token and SCIM tables hold foreign keys into its user table.
pub const SCRIPTS: &[&str] = &[
    account_linking::SCHEMA_SQL,
    SESSIONS_SCHEMA_SQL,
    PASSWORD_SCHEMA_SQL,
    PASSWORD_RESET_SCHEMA_SQL,
    EMAIL_VERIFICATION_SCHEMA_SQL,
    PG_OTP_SCHEMA_SQL,
    PG_MFA_SCHEMA_SQL,
    PG_SCIM_SCHEMA_SQL,
];

/// The auth DDL behind the `auth-saml` feature: the assertion replay cache and the
/// per-tenant identity-provider store.
#[cfg(feature = "auth-saml")]
pub const SAML_SCRIPTS: &[&str] = &[
    crate::saml::PG_SAML_REPLAY_SCHEMA_SQL,
    crate::saml::PG_SAML_IDP_SCHEMA_SQL,
];

#[cfg(test)]
#[path = "auth_schema_tests.rs"]
mod tests;
