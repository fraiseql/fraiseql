//! Full studio wiring check
//!
//! These tests verify the complete studio setup:
//! - HTML shell contains expected Luxen UI markup
//! - All section modules export their response types (compile-time check)
//! - All admin API endpoint handlers are accessible
//!
//! **Execution engine:** none (structural tests)
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]
#![allow(clippy::missing_panics_doc)]

/// Verify the HTML shell contains all required Luxen UI tab items.
#[test]
fn test_studio_shell_has_all_tab_items() {
    use fraiseql_server::routes::studio::studio_shell_html;
    let html = studio_shell_html();
    // Each tab item must reference one of the six sections
    for section in ["data", "auth", "storage", "functions", "realtime", "metrics"] {
        assert!(
            html.contains(&format!("value=\"{section}\"")),
            "Shell must have a tab item for '{section}'"
        );
    }
}

/// Verify the login dialog is present in the shell.
#[test]
fn test_studio_shell_has_login_dialog() {
    use fraiseql_server::routes::studio::studio_shell_html;
    let html = studio_shell_html();
    assert!(html.contains("login-dialog"), "Shell must have a login dialog");
    assert!(html.contains("login-form"), "Shell must have a login form");
    assert!(html.contains("token-input"), "Shell must have a token input");
}

/// Verify the JS asset is loaded as a module (ES module entry point).
#[test]
fn test_studio_shell_loads_js_as_module() {
    use fraiseql_server::routes::studio::studio_shell_html;
    let html = studio_shell_html();
    assert!(
        html.contains("type=\"module\""),
        "app.js must be loaded as an ES module"
    );
}

/// Verify the CSS asset is referenced from the shell.
#[test]
fn test_studio_shell_references_css() {
    use fraiseql_server::routes::studio::studio_shell_html;
    let html = studio_shell_html();
    assert!(html.contains("app.css"), "Shell must reference app.css");
}

/// Verify all admin response types are importable (compile-time only).
#[test]
fn test_all_admin_types_importable() {
    use fraiseql_server::routes::studio::{
        admin::{AdminHealthResponse, AdminSchemaResponse, extract_bearer_token},
        auth_users::{UserInviteRequest, UserListResponse},
        data::{DataBrowserQuery, DataQueryResponse},
        function_ops::{FunctionListResponse, SecretSetRequest},
        metrics_summary::MetricsSummary,
        realtime_monitor::RealtimeStatsResponse,
        storage_browser::{ObjectListResponse, PresignRequest},
    };

    // Instantiate each type to ensure they're fully wired
    let _ = extract_bearer_token(Some("Bearer test"));
    let _ = AdminHealthResponse {
        uptime_secs:    0,
        version:        String::new(),
        pool_active:    0,
        pool_idle:      0,
        pool_max:       0,
        cache_hit_rate: None,
        cache_entries:  None,
    };
    let _ = AdminSchemaResponse { schema: serde_json::Value::Null };
    let _ = UserListResponse { users: vec![], total: 0, page: 1, page_size: 50 };
    let _ = UserInviteRequest { email: String::new() };
    let _ = DataQueryResponse { rows: vec![], total: 0, page: 1, page_size: 50 };
    let _ = DataBrowserQuery {
        page:      1,
        page_size: 50,
        filter:    vec![],
        sort:      vec![],
    };
    let _ = FunctionListResponse { functions: vec![] };
    let _ = SecretSetRequest { value: String::new() };
    let _ = MetricsSummary::zero();
    let _ = RealtimeStatsResponse {
        connections:    0,
        channels:       vec![],
        presence_rooms: vec![],
        cdc_lag_ms:     None,
    };
    let _ = ObjectListResponse { objects: vec![], total: 0, page: 1, page_size: 50 };
    let _ = PresignRequest { bucket: String::new(), key: String::new(), expires_in_secs: 0 };
}

/// Verify `GET /studio` path is distinct from `GET /studio/assets/{file}`.
#[test]
fn test_studio_routes_are_distinct() {
    // The shell fallback must NOT intercept /studio/assets/* — Axum's
    // more-specific `/studio/assets/{file}` route takes priority.
    // This test is a compile-time proof that both handlers exist.
    use fraiseql_server::routes::studio::{studio_asset_handler, studio_handler};
    let _: fn() -> _ = studio_handler;
    let _: fn(_: axum::extract::Path<String>) -> _ = studio_asset_handler;
}
