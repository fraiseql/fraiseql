//! RBAC Management API unit tests.
//!
//! The behavioural coverage for this module lives in
//! `crates/fraiseql-server/tests/rbac_admin_e2e_pg.rs`, against a real PostgreSQL.
//! It has to: every defect this module carried (#748 a DDL parse error, #769 inert
//! tenant scoping and silent truncation, #768 an audit endpoint with no writer) is
//! only observable as a side effect in a database.
//!
//! This file previously held ~90 `#[test]` functions with **empty bodies** — a
//! comment describing the assertion and nothing else — across four files. They
//! reported green for the entire life of the subsystem while its schema DDL did not
//! parse. They are deleted rather than extended: a stub test is worse than no test,
//! because it occupies the name a real one would have taken.
//!
//! What remains here is what genuinely runs without a database.

// ── db_backend_tests ──────────────────────────────────────────────────────────

mod db_backend_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::super::db_backend::*;

    #[test]
    fn test_parse_permission_valid() {
        let (resource, action) = parse_permission("content:write").unwrap();
        assert_eq!(resource, "content");
        assert_eq!(action, "write");
    }

    #[test]
    fn test_parse_permission_wildcard() {
        let (resource, action) = parse_permission("*:*").unwrap();
        assert_eq!(resource, "*");
        assert_eq!(action, "*");
    }

    /// A malformed permission string is the *caller's* error, not the database's.
    ///
    /// It used to be a `QueryError`, which the handler mapped to
    /// `409 role_duplicate` alongside a dead database (#769).
    #[test]
    fn test_parse_permission_invalid() {
        assert!(
            matches!(parse_permission("no_colon"), Err(RbacDbError::InvalidInput(_))),
            "expected InvalidInput for permission without colon, got: {:?}",
            parse_permission("no_colon")
        );
    }

    #[test]
    fn test_rbac_db_error_display() {
        assert_eq!(format!("{}", RbacDbError::RoleNotFound), "Role not found");
        assert_eq!(format!("{}", RbacDbError::RoleDuplicate), "Role already exists");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code; pool ctor errors must panic to surface test setup failures
mod router_construction {
    //! See `crates/fraiseql-server/src/observers/routes.rs::tests` for context:
    //! axum validates path-capture syntax inside `Router::route`, so any
    //! lingering `:param` literal panics here at build time (issue #316).

    use std::sync::Arc;

    use sqlx::PgPool;

    use crate::api::rbac_management::{
        RbacManagementState, db_backend::RbacDbBackend, rbac_management_router,
    };

    fn lazy_pool() -> PgPool {
        PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap()
    }

    #[tokio::test]
    async fn rbac_router_constructs() {
        let state = RbacManagementState {
            db: Arc::new(RbacDbBackend::new(lazy_pool())),
        };
        let _ = rbac_management_router(state);
    }
}
