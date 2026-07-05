//! Tests for the suppression admin router.
//!
//! A `router_construction` test that actually builds the router under
//! `#[tokio::test]` — axum validates path-capture syntax inside `Router::route`,
//! so a bad literal panics here in `cargo test` rather than at first server boot
//! (see the "Bumping axum" gate in `.claude/CLAUDE.md`).

#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;

use super::{SuppressionAdminState, suppression_admin_router};
use crate::inbound::email::PgSendTracker;

mod router_construction {
    use super::*;

    #[tokio::test]
    async fn suppression_admin_router_constructs() {
        // A lazy pool never connects — construction only exercises the route
        // syntax, not the database.
        let pool = PgPoolOptions::new().connect_lazy("postgres://user@localhost/db").unwrap();
        let tracker = Arc::new(PgSendTracker::new(pool));
        let key: Arc<[u8]> = Arc::from(b"key".as_slice());
        let state = Arc::new(SuppressionAdminState::new(tracker, key));
        let _router = suppression_admin_router(state);
    }
}
