//! Security tests for FraiseQL server.
//!
//! Covers authentication bypass detection, privilege escalation prevention,
//! RBAC regression tests, field-level auth edge cases, PKCE flow, and
//! OIDC provider integration.

mod security {
    mod auth_bypass_detection_test;
    mod auth_pkce_flow_test;
    mod field_auth_edge_cases_test;
    mod oidc_provider_integration_test;
    mod privilege_escalation_test;
    mod rbac_auth_regression_test;
}
