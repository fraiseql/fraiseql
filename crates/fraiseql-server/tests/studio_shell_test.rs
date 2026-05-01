//! Cycle 1 tests: SPA shell + embedded assets for /studio
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions

use fraiseql_server::routes::studio::studio_shell_html;

/// GET /studio must return HTML that contains the Luxen UI tab component.
#[test]
fn test_studio_shell_contains_l_tabs() {
    let html = studio_shell_html();
    assert!(
        html.contains("<l-tabs"),
        "Studio shell must contain <l-tabs> for section navigation"
    );
}

/// Shell must contain all six section names.
#[test]
fn test_studio_shell_contains_all_sections() {
    let html = studio_shell_html();
    for section in [
        "Data",
        "Auth",
        "Storage",
        "Functions",
        "Realtime",
        "Metrics",
    ] {
        assert!(html.contains(section), "Studio shell must contain section '{section}'");
    }
}

/// Shell must reference the bundled JS asset.
#[test]
fn test_studio_shell_references_app_js() {
    let html = studio_shell_html();
    assert!(
        html.contains("app.js"),
        "Studio shell must reference the bundled JavaScript asset"
    );
}

/// Embedded asset map must contain app.js.
#[test]
fn test_studio_assets_contain_app_js() {
    use fraiseql_server::routes::studio::StudioAssets;

    let asset = StudioAssets::get("app.js");
    assert!(asset.is_some(), "StudioAssets must contain app.js");
}

/// app.js asset must have non-empty content.
#[test]
fn test_studio_app_js_is_non_empty() {
    use fraiseql_server::routes::studio::StudioAssets;

    let asset = StudioAssets::get("app.js").expect("app.js must exist in StudioAssets");
    assert!(!asset.data.is_empty(), "app.js must have non-empty content");
}
