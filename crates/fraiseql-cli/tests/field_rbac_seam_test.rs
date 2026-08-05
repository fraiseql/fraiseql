//! #757: `[fraiseql.security]` role definitions were compiled as camelCase keys the
//! runtime deserializes as `snake_case`, so **every field-level RBAC grant silently
//! evaporated**.
//!
//! `SecurityConfig::to_json` emitted `roleDefinitions` and `defaultRole`, and
//! `TenancyTomlConfig::to_json` emitted `tenantClaim`. `fraiseql_core`'s
//! `SecurityConfig`/`TenancyConfig` declare `role_definitions`, `default_role` and
//! `tenant_claim` with no aliases and a `#[serde(flatten)] additional` catch-all — so
//! the camelCase keys landed in that untyped map, nothing ever read them out, and the
//! typed fields kept their defaults.
//!
//! The consequence is a **deny-all** field-RBAC surface:
//! `apply_field_rbac_filtering` → `can_access_scope` → `role_has_scope` consults
//! `role_definitions` and nothing else, so with it permanently empty no role grants
//! any scope. An operator who declares `role_definitions = [{name="hr", scopes=[…]}]`
//! and marks a field `requires_scope` gets `Access denied` for members of `hr`.
//! `tenant_claim` was the same shape in the other direction: compile-time
//! `@tenant_id` validation reads the camelCase key while the runtime reads the
//! `snake_case` one, so the two disagreed about which claim carries the tenant.
//!
//! **These tests ask the consumer's question of a compiled file, through the real
//! binary.** Asserting that `security.roleDefinitions` is present in the JSON is
//! exactly what would have passed against the broken code: the key *was* there. The
//! only question that matters is whether the runtime struct that gates access can
//! see it.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{fs, process::Command};

use fraiseql_core::{schema::CompiledSchema, security::SecurityContext};
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

/// A project `fraiseql.toml` (`[fraiseql.*]`) applied over a JSON schema whose
/// `salary` field is scope-gated — the exact shape from the issue's repro.
fn project_dir(fraiseql_extra: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("schema.json"),
        r#"{
  "types": [{"name": "Employee", "fields": [
    {"name": "id", "type": "ID", "nullable": false},
    {"name": "salary", "type": "Int", "nullable": true, "requires_scope": "read:Employee.salary"}
  ]}],
  "queries": [{"name": "employees", "return_type": "Employee", "returns_list": true,
               "sql_source": "v_employee"}]
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

const HR_ROLE: &str = r#"
[[fraiseql.security.role_definitions]]
name = "hr"
description = "Human resources"
scopes = ["read:Employee.salary"]
"#;

// ── The grant must survive the compile ────────────────────────────────────────

/// The typed field the runtime reads must be populated, not the untyped catch-all.
#[test]
fn role_definitions_reach_the_runtime_security_config() {
    let dir = project_dir(HR_ROLE);
    let schema = compile(&dir, &["schema.json"]);

    let security = schema.security.as_ref().expect("compiled schema carries a security section");
    assert!(
        !security.role_definitions.is_empty(),
        "#757: `[[fraiseql.security.role_definitions]]` must land in the typed field the \
         runtime reads (the untyped catch-all is gone since #977)"
    );
    assert_eq!(security.role_definitions[0].name, "hr");
}

/// The question the runtime actually asks. This is the assertion that matters:
/// `role_has_scope` is the sole input to field-level access, so a grant that does not
/// answer `true` here is a grant that does not exist.
#[test]
fn a_declared_role_grants_its_scope_at_runtime() {
    let dir = project_dir(HR_ROLE);
    let schema = compile(&dir, &["schema.json"]);
    let security = schema.security.as_ref().expect("security section");

    assert!(
        security.role_has_scope("hr", "read:Employee.salary"),
        "#757: the declared grant must be visible to `role_has_scope`, which is the only \
         thing `can_access_scope` consults"
    );

    // And through the full path a request takes.
    let ctx =
        SecurityContext::service_account("alice", "req-1", vec!["hr".to_string()], vec![], None);
    assert!(
        ctx.can_access_scope(security, "read:Employee.salary"),
        "#757: a member of `hr` must be able to read the field `hr` was granted"
    );
}

/// Counterweight: the grant must be a grant, not a blanket allow. Without this, a
/// `role_has_scope` that returned `true` unconditionally would pass the case above.
#[test]
fn an_undeclared_role_grants_nothing() {
    let dir = project_dir(HR_ROLE);
    let schema = compile(&dir, &["schema.json"]);
    let security = schema.security.as_ref().expect("security section");

    assert!(!security.role_has_scope("intern", "read:Employee.salary"));
    assert!(!security.role_has_scope("hr", "write:Employee.salary"));

    let ctx =
        SecurityContext::service_account("bob", "req-2", vec!["intern".to_string()], vec![], None);
    assert!(!ctx.can_access_scope(security, "read:Employee.salary"));
}

/// `default_role` must survive too — it is part of the same `to_json` block and was
/// dropped by the same rename.
#[test]
fn default_role_reaches_the_runtime_security_config() {
    let dir = project_dir(&format!("[fraiseql.security]\ndefault_role = \"viewer\"\n{HR_ROLE}"));
    let schema = compile(&dir, &["schema.json"]);
    let security = schema.security.as_ref().expect("security section");
    assert_eq!(
        security.default_role.as_deref(),
        Some("viewer"),
        "#757: `default_role` must land in the typed field, not the catch-all"
    );
}

/// `tenant_claim` is the third key in the cluster, and the one where compile time and
/// run time actively disagreed: `@tenant_id` validation read `tenantClaim` while the
/// runtime read `tenant_claim` and silently fell back to its `"tenant_id"` default.
#[test]
fn tenant_claim_reaches_the_runtime_tenancy_config() {
    let dir = project_dir("[fraiseql.tenancy]\nmode = \"schema\"\ntenant_claim = \"org_id\"");
    let schema = compile(&dir, &["schema.json"]);
    let tenancy = schema.tenancy_config().expect("compiled schema carries a tenancy section");
    assert_eq!(
        tenancy.tenant_claim, "org_id",
        "#757: a configured `tenant_claim` must reach the runtime instead of reverting to \
         the default"
    );
}

/// No camelCase spelling of these keys may survive into the compiled artifact.
///
/// The three keys are fixed here rather than left to convention: this is the seam
/// that has now produced #755, #756, #757, #806, #807 and #847, and every one of them
/// was a producer and a consumer disagreeing about a name while both looked correct
/// in isolation.
#[test]
fn no_camel_case_security_keys_survive_the_compile() {
    let dir = project_dir(&format!(
        "[fraiseql.security]\ndefault_role = \"viewer\"\n{HR_ROLE}\n\
         [fraiseql.tenancy]\nmode = \"row\"\ntenant_claim = \"org_id\"\n"
    ));
    let out = dir.path().join("schema.compiled.json");
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["compile", "schema.json", "--output", out.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .expect("run fraiseql-cli");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));

    let raw = fs::read_to_string(&out).expect("read compiled schema");
    for camel in ["roleDefinitions", "defaultRole", "tenantClaim"] {
        assert!(
            !raw.contains(camel),
            "#757: `{camel}` must not be emitted — the runtime reads snake_case, so a \
             camelCase key is an authorization grant that silently does not exist"
        );
    }
}
