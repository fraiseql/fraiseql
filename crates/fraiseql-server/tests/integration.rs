//! Integration tests for FraiseQL server requiring live infrastructure.
//!
//! These tests exercise the full server stack — HTTP routing, database queries,
//! and end-to-end GraphQL execution — against real services.

mod integration {
    mod database_integration_test;
    mod server_e2e_test;
}
