//! Studio admin dashboard routes.
//!
//! Serves the FraiseQL Studio SPA at `/studio` with:
//! - `GET /studio` — HTML shell (client-side routing entry point)
//! - `GET /studio/assets/{file}` — bundled JS/CSS assets
//! - `GET /studio/*path` — wildcard fallback returning the same HTML shell

use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use rust_embed::RustEmbed;

/// Embedded studio assets bundled by `build.rs` via esbuild.
///
/// The folder path uses an environment variable set by `build.rs`
/// so the path resolves correctly during both normal builds and tests.
#[derive(RustEmbed)]
#[folder = "$FRAISEQL_STUDIO_DIST"]
pub struct StudioAssets;

/// Returns the HTML shell for the Studio SPA.
///
/// The shell bootstraps Luxen UI tab navigation and loads the bundled JS.
/// All section content is fetched from `/admin/v1/*` at runtime.
#[must_use]
pub fn studio_shell_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>FraiseQL Studio</title>
  <link rel="stylesheet" href="/studio/assets/app.css" />
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    html, body {
      margin: 0;
      height: 100%;
      font-family: system-ui, sans-serif;
      background: var(--color-surface, #f8f8f8);
      color: var(--color-text, #1a1a1a);
    }
    #app {
      display: flex;
      flex-direction: column;
      height: 100%;
    }
    header {
      display: flex;
      align-items: center;
      gap: 1rem;
      padding: 0.75rem 1.5rem;
      background: var(--color-surface-raised, #fff);
      border-bottom: 1px solid var(--color-border, #e2e2e2);
    }
    header h1 {
      font-size: 1rem;
      font-weight: 600;
      margin: 0;
    }
    #section-content {
      flex: 1;
      padding: 1.5rem;
      overflow: auto;
    }
    .section-placeholder { padding: 1rem 0; }
    .empty-state {
      color: var(--color-text-secondary, #666);
      font-style: italic;
    }
    .log-viewer {
      background: #1e1e1e;
      color: #d4d4d4;
      font-family: "Cascadia Code", "Fira Code", monospace;
      font-size: 0.8rem;
      padding: 1rem;
      border-radius: 6px;
      overflow: auto;
      max-height: 70vh;
      white-space: pre;
    }
  </style>
</head>
<body>
  <div id="app">
    <header>
      <h1>FraiseQL Studio</h1>
      <l-badge variant="neutral" id="version-badge">runtime</l-badge>
    </header>

    <l-tabs value="data" style="border-bottom:1px solid var(--color-border,#e2e2e2)">
      <l-tab-item value="data">Data</l-tab-item>
      <l-tab-item value="auth">Auth</l-tab-item>
      <l-tab-item value="storage">Storage</l-tab-item>
      <l-tab-item value="functions">Functions</l-tab-item>
      <l-tab-item value="realtime">Realtime</l-tab-item>
      <l-tab-item value="metrics">Metrics</l-tab-item>
    </l-tabs>

    <div id="section-content">
      <l-spinner></l-spinner>
    </div>
  </div>

  <!-- Login dialog -->
  <dialog id="login-dialog" style="border:none;border-radius:8px;padding:2rem;min-width:320px">
    <h2 style="margin-top:0">Admin login</h2>
    <form id="login-form">
      <label for="token-input" style="display:block;margin-bottom:0.5rem;font-size:0.9rem">Admin token</label>
      <input id="token-input" type="password" placeholder="Bearer token" autocomplete="off"
             style="width:100%;padding:0.5rem;border:1px solid #ccc;border-radius:4px;font-size:1rem;margin-bottom:1rem" />
      <button type="submit"
              style="width:100%;padding:0.6rem;background:#0070f3;color:#fff;border:none;border-radius:4px;font-size:1rem;cursor:pointer">
        Sign in
      </button>
    </form>
  </dialog>

  <l-toast id="login-toast"></l-toast>

  <script src="/studio/assets/app.js" type="module"></script>
</body>
</html>"#.to_owned()
}

/// Handler for `GET /studio` and `GET /studio/*path` — serves the SPA shell.
///
/// All sub-paths return the same HTML shell to support client-side routing.
pub async fn studio_handler() -> impl IntoResponse {
    Html(studio_shell_html())
}

/// Handler for `GET /studio/assets/{file}` — serves embedded static assets.
///
/// # Errors
///
/// Returns `404 Not Found` if the requested asset does not exist in the embedded bundle.
pub async fn studio_asset_handler(Path(file): Path<String>) -> Response {
    match StudioAssets::get(&file) {
        Some(asset) => {
            let mime = mime_for_filename(&file);
            ([(header::CONTENT_TYPE, mime)], asset.data.as_ref().to_vec()).into_response()
        },
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Derive MIME type from file extension.
fn mime_for_filename(name: &str) -> &'static str {
    let ext = std::path::Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("js") {
        "application/javascript; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("css") {
        "text/css; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("map") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_contains_l_tabs() {
        let html = studio_shell_html();
        assert!(html.contains("<l-tabs"), "shell must contain <l-tabs>");
    }

    #[test]
    fn test_shell_contains_all_sections() {
        let html = studio_shell_html();
        for s in [
            "Data",
            "Auth",
            "Storage",
            "Functions",
            "Realtime",
            "Metrics",
        ] {
            assert!(html.contains(s), "shell must contain section '{s}'");
        }
    }

    #[test]
    fn test_shell_references_app_js() {
        let html = studio_shell_html();
        assert!(html.contains("app.js"), "shell must reference app.js");
    }

    #[test]
    fn test_mime_for_js() {
        assert!(mime_for_filename("app.js").contains("javascript"));
    }

    #[test]
    fn test_mime_for_css() {
        assert!(mime_for_filename("app.css").contains("css"));
    }
}
