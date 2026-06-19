//! Unit tests for `fraiseql watch` helpers (pure; no server or filesystem).

use super::reload_endpoint;

#[test]
fn reload_endpoint_appends_admin_path() {
    assert_eq!(
        reload_endpoint("http://localhost:8080"),
        "http://localhost:8080/api/v1/admin/reload-schema"
    );
}

#[test]
fn reload_endpoint_tolerates_trailing_slash() {
    // A trailing slash on the base must not produce a double slash.
    assert_eq!(
        reload_endpoint("http://localhost:8080/"),
        "http://localhost:8080/api/v1/admin/reload-schema"
    );
}
