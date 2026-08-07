//! #758: the two gates that depend on `security.multi_tenant` could never fire,
//! because nothing in any compile path could set it.
//!
//! `CompiledSchema::is_multi_tenant()` read `security.multi_tenant`. Both TOML
//! security structs are `deny_unknown_fields`, so `multi_tenant = true` was a hard
//! compile error; no SDK emitted the key either. The result: the subscription
//! tenant fail-closed gate (`_ => !self.multi_tenant`) was permanently permissive,
//! and the cache+RLS boot gate — whose error text tells the operator to "set
//! `[security] multi_tenant = false`" — always took the warn-only branch.
//!
//! `has_rls_configured()` was the same shape one level down: it counted
//! `security.additional["policies"]`, and #612 made a non-empty `[security.policies]`
//! a hard compile error, so it too was `false` for every schema any supported
//! workflow could produce.
//!
//! **These tests drive the real `fraiseql-cli` binary** and then ask the *consumer's*
//! questions — `is_multi_tenant()`, `has_rls_configured()`, `session_variables` — of
//! a `CompiledSchema` deserialized from the emitted file. Asserting on raw JSON keys
//! instead would pass under exactly the camelCase drift (#757 class) that this seam
//! keeps producing: a key can be present in the file and still be invisible to the
//! runtime that reads it.
//!
//! Both compile paths are covered, because they have **separate** security
//! producers: `schema/merger.rs` for a TOML schema, and `config/security.rs`'s
//! `to_json` for a project `fraiseql.toml` applied over a JSON schema.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{fs, process::Command};

use fraiseql_core::schema::{CompiledSchema, SessionVariableSource, TenancyMode};
use tempfile::TempDir;

/// Compile via the real binary and deserialize the result the way the server does.
fn compile(dir: &TempDir, args: &[&str]) -> CompiledSchema {
    let out = dir.path().join("schema.compiled.json");
    let mut argv = vec!["compile"];
    argv.extend_from_slice(args);
    argv.extend_from_slice(&["--output", out.to_str().unwrap()]);

    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(&argv)
        .current_dir(dir.path())
        .output()
        .expect("run fraiseql-cli");
    assert!(
        result.status.success(),
        "compile failed\nargs: {argv:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let json = fs::read_to_string(&out).expect("read compiled schema");
    CompiledSchema::from_json(&json, false)
        .expect("compiled schema must load as the server loads it")
}

/// Compile expecting failure; returns combined stdout+stderr.
fn compile_err(dir: &TempDir, args: &[&str]) -> String {
    let out = dir.path().join("schema.compiled.json");
    let mut argv = vec!["compile"];
    argv.extend_from_slice(args);
    argv.extend_from_slice(&["--output", out.to_str().unwrap()]);

    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(&argv)
        .current_dir(dir.path())
        .output()
        .expect("run fraiseql-cli");
    assert!(!result.status.success(), "expected the compile to fail, args: {argv:?}");
    format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    )
}

const TYPES_JSON: &str = r#"{
  "types": [
    {"name": "User", "fields": [
      {"name": "id", "type": "ID", "nullable": false},
      {"name": "tenantId", "type": "String", "nullable": false}
    ]}
  ]
}"#;

/// A TOML *schema* (the `fraiseql compile fraiseql.toml --types …` workflow).
fn toml_schema(security_extra: &str) -> String {
    format!(
        r#"
[schema]
name = "tenancy_seam"
version = "1.0.0"
database_target = "postgresql"

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"

[security]
{security_extra}
"#
    )
}

fn toml_schema_dir(security_extra: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), TYPES_JSON).unwrap();
    fs::write(dir.path().join("schema.toml"), toml_schema(security_extra)).unwrap();
    dir
}

/// A project `fraiseql.toml` (`[fraiseql.*]`) applied over a JSON schema.
fn project_config_dir(fraiseql_extra: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("schema.json"),
        r#"{
  "types": [{"name": "User", "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
  "queries": [{"name": "users", "return_type": "User", "returns_list": true, "sql_source": "v_user"}]
}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        format!(
            r"
[fraiseql]
{fraiseql_extra}
"
        ),
    )
    .unwrap();
    dir
}

// ── #758: multi_tenant must be declarable, and tenancy.mode must imply it ──────

#[test]
fn toml_schema_can_declare_multi_tenant() {
    let dir = toml_schema_dir("multi_tenant = true");
    let schema = compile(&dir, &["schema.toml", "--types", "types.json"]);
    assert!(
        schema.is_multi_tenant(),
        "#758: `[security] multi_tenant = true` must reach the runtime — the boot gate's own \
         error message tells operators to set this key"
    );
}

#[test]
fn project_config_can_declare_multi_tenant() {
    let dir = project_config_dir("[fraiseql.security]\nmulti_tenant = true");
    let schema = compile(&dir, &["schema.json"]);
    assert!(
        schema.is_multi_tenant(),
        "#758: `[fraiseql.security] multi_tenant` must reach the runtime"
    );
}

/// The knob operators actually reach for. A deployment that declares row-level
/// tenancy *is* multi-tenant; requiring a second, separate boolean to say so is how
/// the gate ended up permanently off.
#[test]
fn row_tenancy_mode_implies_multi_tenant() {
    let dir =
        project_config_dir("[fraiseql.tenancy]\nmode = \"row\"\ntenant_claim = \"tenant_id\"");
    let schema = compile(&dir, &["schema.json"]);
    assert_eq!(schema.tenancy_mode(), TenancyMode::Row);
    assert!(
        schema.is_multi_tenant(),
        "#758: `[fraiseql.tenancy] mode = \"row\"` is a multi-tenant declaration; the \
         subscription fail-closed gate and the cache+RLS boot gate must both see it"
    );
}

#[test]
fn schema_tenancy_mode_implies_multi_tenant() {
    let dir = project_config_dir("[fraiseql.tenancy]\nmode = \"schema\"");
    let schema = compile(&dir, &["schema.json"]);
    assert_eq!(schema.tenancy_mode(), TenancyMode::Schema);
    assert!(schema.is_multi_tenant());
}

/// Counterweight: a single-tenant schema must stay single-tenant, or the boot gate
/// would simply refuse everything and prove nothing.
#[test]
fn a_plain_schema_is_not_multi_tenant() {
    let dir = toml_schema_dir("");
    let schema = compile(&dir, &["schema.toml", "--types", "types.json"]);
    assert!(!schema.is_multi_tenant());
    assert!(!schema.has_rls_configured());
}

// ── #758 (second half): RLS must be declarable ─────────────────────────────────

#[test]
fn toml_schema_can_declare_rls() {
    let dir = toml_schema_dir("multi_tenant = true\n\n[security.rls]\nenabled = true");
    let schema = compile(&dir, &["schema.toml", "--types", "types.json"]);
    assert!(
        schema.has_rls_configured(),
        "#758: `[security.rls] enabled` must reach the runtime — the boot gate's error text \
         has been telling operators to `declare [security.rls] policies` for a section that \
         did not exist"
    );
}

#[test]
fn project_config_can_declare_rls() {
    let dir = project_config_dir("[fraiseql.security.rls]\nenabled = true");
    let schema = compile(&dir, &["schema.json"]);
    assert!(schema.has_rls_configured());
}

/// `[security.policies]` is rejected outright since #612, so a `has_rls_configured`
/// that counts it can only ever answer `false`. The replacement must not quietly
/// keep reading it.
#[test]
fn declaring_authorization_policies_is_still_refused() {
    let dir = toml_schema_dir(
        "[[security.policies]]\nname = \"p\"\ntype = \"rbac\"\nrule = \"true\"\nroles = []",
    );
    let err = compile_err(&dir, &["schema.toml", "--types", "types.json"]);
    assert!(err.contains("security.policies"), "expected the #612 refusal, got: {err}");
}

// ── #628 groundwork: session variables must be declarable in TOML ─────────────

/// The compiled-schema field's own doc says "Compiled from the `[session_variables]`
/// TOML section". No such section existed in either TOML format, so the only way to
/// declare the mechanism RLS policies read was to hand-author `schema.json`.
#[test]
fn toml_schema_can_declare_session_variables() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), TYPES_JSON).unwrap();
    fs::write(
        dir.path().join("schema.toml"),
        format!(
            "{}\n{}",
            toml_schema("multi_tenant = true\n\n[security.rls]\nenabled = true"),
            r#"
[session_variables]
inject_started_at = false

[[session_variables.variables]]
name = "app.tenant_id"
source = "jwt"
claim = "tenant_id"
"#
        ),
    )
    .unwrap();

    let schema = compile(&dir, &["schema.toml", "--types", "types.json"]);
    let vars = &schema.session_variables;
    assert!(!vars.inject_started_at);
    assert_eq!(vars.variables.len(), 1, "{vars:?}");
    assert_eq!(vars.variables[0].name, "app.tenant_id");
    assert_eq!(
        vars.variables[0].source,
        SessionVariableSource::Jwt {
            claim: "tenant_id".to_string(),
        },
        "#628: the JWT claim → PostgreSQL session variable mapping is what an RLS policy \
         reads with current_setting(); it must survive compilation"
    );
}

#[test]
fn project_config_can_declare_session_variables() {
    let dir = project_config_dir(
        "[fraiseql.session_variables]\n\
         inject_started_at = false\n\n\
         [[fraiseql.session_variables.variables]]\n\
         name = \"app.tenant_id\"\n\
         source = \"jwt\"\n\
         claim = \"tenant_id\"",
    );
    let schema = compile(&dir, &["schema.json"]);
    assert_eq!(schema.session_variables.variables.len(), 1);
    assert_eq!(schema.session_variables.variables[0].name, "app.tenant_id");
}

/// A malformed source must be refused, not silently dropped into an empty default —
/// the failure mode this whole seam keeps reproducing.
#[test]
fn a_session_variable_with_an_unknown_source_is_refused() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), TYPES_JSON).unwrap();
    fs::write(
        dir.path().join("schema.toml"),
        format!(
            "{}\n{}",
            toml_schema(""),
            r#"
[[session_variables.variables]]
name = "app.tenant_id"
source = "telepathy"
claim = "tenant_id"
"#
        ),
    )
    .unwrap();
    let err = compile_err(&dir, &["schema.toml", "--types", "types.json"]);
    assert!(
        err.contains("telepathy") || err.to_lowercase().contains("unknown variant"),
        "expected a loud refusal naming the bad source, got: {err}"
    );
}
