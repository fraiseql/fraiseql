//! Security tests for FraiseQL core.
//!
//! Covers SQL injection prevention, Row-Level Security enforcement,
//! field-level RBAC, and path/identifier injection hardening.

mod security {
    // SQL injection prevention
    mod path_injection_tests;
    mod security_sql_identifier_test;
    mod tenancy_sql_injection_test;
    mod where_sql_injection_prevention;

    // End-to-end injection and RBAC pipeline
    mod e2e_field_rbac_pipeline;
    mod e2e_sql_injection_integration;

    // Row-Level Security and Field RBAC integration
    mod integration_executor_field_rbac;
    mod integration_field_rbac_errors;
    mod integration_field_rbac_runtime;
    mod integration_field_rbac_toml;
    mod integration_rls;
}
