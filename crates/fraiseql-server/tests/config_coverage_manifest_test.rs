#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Config-coverage drift gate for the server runtime surface (#612 item M).
//!
//! The CLI-side twin lives in `fraiseql-cli/tests/config_coverage_manifest_test.rs`
//! and guards the authoring/compiled surface; this one guards `ServerConfig` — the
//! runtime config the server initialises from the compiled schema + env overrides.
//! It walks every leaf of `ServerConfig::default()` and asserts each is owned by a
//! named subsystem in the checked-in [`MANIFEST`]. A new `ServerConfig` field that
//! no subsystem consumes (the #612 defect class, server-side) fails at PR time.
//!
//! Limitation: `serde` omits `Option::None` / `skip_serializing_if`-empty fields
//! from `default()`, so an entirely-absent subtree is not walked here.

use fraiseql_server::server_config::ServerConfig;
use serde_json::Value;

/// Every leaf key-path of `ServerConfig::default()` mapped to its consuming
/// subsystem. `ServerConfig` uses flat `snake_case` keys, so a trailing `*` is a
/// plain-prefix glob (`metrics*` owns `metrics_enabled`, `metrics_path`, …); a
/// bare path is an exact leaf.
const MANIFEST: &[(&str, &str)] = &[
    // ── Bind / transport / TLS ───────────────────────────────────────────────
    ("bind_addr", "server HTTP bind address"),
    ("cors_enabled", "CORS layer toggle"),
    ("cors_origins", "CORS allowed origins"),
    ("compression_enabled", "response compression layer"),
    ("require_json_content_type", "request content-type guard"),
    ("tls", "server TLS config (Option — HTTPS listener)"),
    ("database_tls", "database TLS config (Option)"),
    // ── Routes / endpoints ───────────────────────────────────────────────────
    ("graphql_path", "GraphQL route mount path"),
    ("health_path", "health endpoint path"),
    ("readiness_path", "readiness endpoint path"),
    ("introspection_path", "introspection endpoint path"),
    ("subscription_path", "subscription (WS) route path"),
    ("playground_path", "GraphQL playground path"),
    ("playground_enabled", "playground toggle"),
    ("playground_tool", "playground UI selection (GraphiQL/…)"),
    ("metrics_json_path", "JSON metrics endpoint path"),
    // ── Feature toggles ──────────────────────────────────────────────────────
    ("apq_enabled", "automatic persisted queries"),
    ("cache_enabled", "query result cache"),
    ("subscriptions_enabled", "subscriptions runtime toggle"),
    ("introspection_enabled", "introspection enforcer (#455)"),
    ("introspection_require_auth", "introspection auth gate"),
    ("validate_sql_sources", "compile-time SQL-source validation"),
    // ── Metrics / tracing ────────────────────────────────────────────────────
    ("metrics*", "server metrics (metrics-exporter-prometheus; enabled/path/token)"),
    ("tracing_enabled", "OTLP tracing toggle"),
    ("tracing_service_name", "OTLP service name"),
    ("otlp_endpoint", "OTLP exporter endpoint (Option)"),
    ("otlp_export_timeout_secs", "OTLP export timeout"),
    // ── Admin API ────────────────────────────────────────────────────────────
    ("admin_api_enabled", "admin API mount toggle"),
    ("admin_token", "admin API bearer token (Option)"),
    ("admin_readonly_token", "admin API read-only token (Option)"),
    ("admin_auth_max_failures", "admin auth brute-force lockout threshold"),
    ("design_api_require_auth", "design API auth gate"),
    // ── Auth backends ────────────────────────────────────────────────────────
    ("auth", "OIDC auth config (fraiseql-core OidcConfig, Option)"),
    ("auth_hs256", "HS256 JWT auth config (Option)"),
    ("hmac_secret_env", "HMAC secret env var name (Option)"),
    ("identity", "enriched-identity resolver config (Option)"),
    // ── Database pool ────────────────────────────────────────────────────────
    ("database_url", "primary database connection URL"),
    ("pool_min_size", "DB pool min size"),
    ("pool_max_size", "DB pool max size"),
    ("pool_timeout_secs", "DB pool acquire timeout"),
    ("pool_tuning", "pool-pressure monitor config (Option)"),
    // ── Request limits / admission ───────────────────────────────────────────
    ("max_request_body_bytes", "request body size cap"),
    ("max_header_count", "request header count cap"),
    ("max_header_bytes", "request header size cap"),
    ("max_get_query_bytes", "GET query size cap"),
    ("request_timeout_secs", "per-request timeout (Option)"),
    ("shutdown_timeout_secs", "graceful shutdown timeout"),
    ("admission_control", "admission-control / load-shed config (Option)"),
    // ── Optional subsystems ──────────────────────────────────────────────────
    ("rate_limiting", "rate-limit middleware config (Option; #609)"),
    ("observers", "observer runtime config (Option; `observers` feature)"),
    ("storage", "object-storage config (Option; #608)"),
    ("storage_token", "storage admin token (Option)"),
    ("files", "file-serving config (Option)"),
    ("usage", "usage-metering config (Option)"),
    ("tenancy*", "multi-tenant runtime config (tenancy.runtime.enabled)"),
    (
        "validation",
        "schema validation config (Option; fraiseql-core ValidationConfig)",
    ),
    ("schema_path", "compiled schema file path"),
    ("security_contact", "security.txt contact (Option)"),
];

fn collect_leaves(value: &Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect_leaves(v, &path, out);
            }
        },
        _ => out.push(prefix.to_string()),
    }
}

/// True if `pattern` covers `path`. `foo.*` is a dotted-section prefix; a bare
/// trailing `*` (`metrics*`) is a plain string prefix for flat `snake_case` keys;
/// otherwise an exact match.
fn pattern_covers(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix(".*") {
        path == prefix || path.starts_with(&format!("{prefix}."))
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        path.starts_with(prefix)
    } else {
        pattern == path
    }
}

fn is_covered(path: &str) -> bool {
    MANIFEST.iter().any(|(pattern, _)| pattern_covers(pattern, path))
}

#[test]
fn every_server_config_leaf_has_a_named_consumer() {
    let default_json = serde_json::to_value(ServerConfig::default()).unwrap();
    let mut leaves = Vec::new();
    collect_leaves(&default_json, "", &mut leaves);
    leaves.sort();
    leaves.dedup();

    let uncovered: Vec<&String> = leaves.iter().filter(|p| !is_covered(p)).collect();
    assert!(
        uncovered.is_empty(),
        "ServerConfig has runtime leaves with no named consumer (#612 item M). Add each to \
         MANIFEST in {} naming who consumes it:\n{}",
        file!(),
        uncovered
            .iter()
            .map(|p| format!("    (\"{p}\", \"<consumer>\"),"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_manifest_entry_matches_a_real_leaf() {
    let default_json = serde_json::to_value(ServerConfig::default()).unwrap();
    let mut leaves = Vec::new();
    collect_leaves(&default_json, "", &mut leaves);

    let matches_a_leaf =
        |pattern: &str| -> bool { leaves.iter().any(|l| pattern_covers(pattern, l)) };
    let stale: Vec<&str> =
        MANIFEST.iter().map(|(p, _)| *p).filter(|p| !matches_a_leaf(p)).collect();
    assert!(
        stale.is_empty(),
        "manifest entries no longer match any ServerConfig leaf: {stale:?}"
    );
}

#[test]
fn an_unmanifested_key_is_flagged_as_uncovered() {
    assert!(!is_covered("brand_new_knob"), "a new server config key must be uncovered");
}
