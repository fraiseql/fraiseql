#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! #612 config-drift pins — the config-file surface (Phase 04).
//!
//! One test per disposition target from `config-key-disposition.md` that lives on
//! the `fraiseql-cli` TOML/merger surface. Each pin asserts the *disposed* behavior:
//!
//! - **5b** rate-limiting shape — the merger emits the flat `snake_case` `security.rate_limiting`
//!   the live server reader consumes (the dead nested-`camelCase` reader was removed in
//!   `fraiseql-auth`).
//! - **REJECT** rows (#1 `[caching]`, #2 `[analytics]`, #3 `[observability]`, #4
//!   `[security.rules/policies/field_auth]`, #7 `[security.api_keys] storage`) — each
//!   previously-accepted key now fails **loudly at compile** with a targeted message. These replace
//!   the Phase-00 "accepted silently" characterizations.
//!
//! Items #6 (`revoke_all_ttl_secs`) and #9 (`[auth]`) are pinned by their own
//! landing PRs (#612 commit `3d94331` and PR #622 respectively).

use std::io::Write;

use fraiseql_cli::{config::toml_schema::TomlSchema, schema::merger::SchemaMerger};
use tempfile::NamedTempFile;

/// Parse a TOML schema and return the error `validate()` produces (panics if it
/// unexpectedly validates — a REJECT pin must actually reject).
fn validate_err(toml: &str) -> String {
    let schema = TomlSchema::parse_toml(toml).expect("TOML should parse");
    schema
        .validate()
        .expect_err("config should be rejected at validate()")
        .to_string()
}

/// Write TOML content to a temp file and return the handle (kept alive by caller).
fn toml_file(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

// ---------------------------------------------------------------------------
// 5b — rate-limiting shape: merger emits flat snake_case that the live reader reads
// ---------------------------------------------------------------------------

/// The merger lowers `[security.rate_limiting]` into the compiled schema's
/// `security.rate_limiting` object as **flat `snake_case`** — the exact shape the
/// server's live `RateLimitingSecurityConfig`
/// (`fraiseql-server middleware/rate_limit/config.rs`) deserializes. This pins the
/// contract so the two ends cannot silently drift the way #612 item 5b found (a
/// nested-`camelCase` reader that never matched and fell back to hardcoded defaults).
#[test]
fn rate_limiting_is_emitted_as_flat_snake_case_for_the_live_reader() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"

        [security.rate_limiting]
        enabled                 = true
        auth_start_max_requests = 42
        auth_start_window_secs  = 77
        failed_login_max_attempts = 9
    "#,
    );
    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();

    let security = intermediate.security.expect("security section should be emitted");
    let rate_limiting = security
        .get("rate_limiting")
        .expect("compiled schema must carry flat `security.rate_limiting`");

    // Flat snake_case keys carry the configured values (not defaults, not dropped).
    assert_eq!(rate_limiting.get("enabled").and_then(serde_json::Value::as_bool), Some(true));
    assert_eq!(
        rate_limiting.get("auth_start_max_requests").and_then(serde_json::Value::as_u64),
        Some(42),
        "the live server reader keys on flat snake_case `auth_start_max_requests`"
    );
    assert_eq!(
        rate_limiting.get("auth_start_window_secs").and_then(serde_json::Value::as_u64),
        Some(77)
    );
    assert_eq!(
        rate_limiting
            .get("failed_login_max_attempts")
            .and_then(serde_json::Value::as_u64),
        Some(9)
    );

    // The dead nested-camelCase shape must NOT be what the pipeline emits.
    assert!(
        rate_limiting.get("authStart").is_none(),
        "merger must not emit the nested-camelCase shape the removed reader expected"
    );
}

// ---------------------------------------------------------------------------
// REJECT rows — each previously-accepted section now fails loudly at compile
// ---------------------------------------------------------------------------

/// #4 (highest-stakes): declared-but-unenforced authorization. Any of
/// `[security.rules]` / `[security.policies]` / `[security.field_auth]` must be
/// rejected — the runtime pins the authorizers to None, so shipping them is a false
/// security claim. The error names the blocks, says they are unenforced, and links
/// the declarative-authz follow-up (#626).
#[test]
fn declared_authorization_is_rejected_at_compile() {
    let err = validate_err(
        r#"
        [schema]
        name = "t"

        [[security.rules]]
        name = "read_own"
        rule = "user.id == object.owner_id"
    "#,
    );
    assert!(err.contains("[security.rules]"), "err = {err}");
    assert!(err.contains("does NOT enforce"), "err = {err}");
    assert!(err.contains("/issues/626"), "err = {err}");

    // policies and field_auth are rejected on the same footing.
    assert!(
        validate_err(
            r#"
        [schema]
        name = "t"
        [[security.policies]]
        name = "admin_only"
        type = "rbac"
        roles = ["admin"]
    "#,
        )
        .contains("does NOT enforce")
    );
}

/// #1 `[caching]` — accepted but never lowered into the compiled schema.
#[test]
fn caching_section_is_rejected_at_compile() {
    let err = validate_err(
        r#"
        [schema]
        name = "t"

        [caching]
        enabled = true
        backend = "redis"
    "#,
    );
    assert!(err.contains("[caching]"), "err = {err}");
    assert!(err.contains("/issues/623"), "err = {err}");
}

/// #2 `[analytics]` — fully inert.
#[test]
fn analytics_section_is_rejected_at_compile() {
    let err = validate_err(
        r#"
        [schema]
        name = "t"

        [analytics]
        enabled = true
    "#,
    );
    assert!(err.contains("[analytics]"), "err = {err}");
    assert!(err.contains("/issues/624"), "err = {err}");
}

/// #3 `[observability]` — inert on the compiled path; points to `[metrics]`/`[tracing]`.
#[test]
fn observability_section_is_rejected_at_compile() {
    let err = validate_err(
        r#"
        [schema]
        name = "t"

        [observability]
        prometheus_enabled = true
    "#,
    );
    assert!(err.contains("[observability]"), "err = {err}");
    assert!(err.contains("[metrics]") && err.contains("[tracing]"), "err = {err}");
    assert!(err.contains("/issues/625"), "err = {err}");
}

/// #7 `[security.api_keys] storage` — only `env` is implemented; `postgres` authenticates nothing.
#[test]
fn api_keys_non_env_storage_is_rejected_at_compile() {
    let err = validate_err(
        r#"
        [schema]
        name = "t"

        [security.api_keys]
        enabled = true
        storage = "postgres"
    "#,
    );
    assert!(err.contains("[security.api_keys]"), "err = {err}");
    assert!(err.contains("postgres"), "err = {err}");
    assert!(err.contains("/issues/627"), "err = {err}");

    // The implemented value (`env`) still validates.
    let schema = TomlSchema::parse_toml(
        r#"
        [schema]
        name = "t"

        [security.api_keys]
        enabled = true
        storage = "env"
    "#,
    )
    .expect("TOML should parse");
    schema.validate().expect("storage = \"env\" must remain valid");
}

/// #8 `[[observers.handlers]]` — compiled but never run at runtime. Runtime
/// observers come only from `tb_observer` / the admin observer API, so a
/// TOML-declared handler silently never fires. Rejected at compile with a pointer
/// to the load-at-boot follow-up (#631). `[observers] enabled` (a consumed
/// changelog gate) without handlers must still validate.
#[test]
fn compiled_observer_handlers_are_rejected_at_compile() {
    let err = validate_err(
        r#"
        [schema]
        name = "t"

        [[observers.handlers]]
        name = "notify"
        event = "INSERT"
        action = "webhook"
        webhook_url = "https://hook.example/x"
    "#,
    );
    assert!(err.contains("[[observers.handlers]]"), "err = {err}");
    assert!(err.contains("tb_observer"), "err = {err}");
    assert!(err.contains("/issues/631"), "err = {err}");

    // `[observers] enabled` without handlers is a consumed changelog gate — still valid.
    let schema = TomlSchema::parse_toml(
        r#"
        [schema]
        name = "t"

        [observers]
        enabled = true
    "#,
    )
    .expect("TOML should parse");
    schema
        .validate()
        .expect("[observers] enabled without handlers must remain valid");
}
