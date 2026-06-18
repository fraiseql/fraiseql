#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! Compiler → runtime contract gate (audit coverage-gap §2).
//!
//! `fraiseql-cli` emits `schema.compiled.json`; the server/core parse it back with
//! serde. A field added on one side and silently defaulted (or refused) on the other
//! passes each crate's own lens — this class has caused two production REDs
//! (`token_revocation: null` boot-refusal; serde-default divergences). These tests
//! compile real fixtures with the CLI binary and assert that:
//!
//! 1. `RuntimeConfig::from_compiled_schema` — the server's boot-time config seam — accepts the
//!    compiler's output (it must not refuse valid compiler output), and
//! 2. an enterprise security toggle set in `fraiseql.toml` survives the full emit → parse →
//!    runtime-config-derivation chain (proving the field is *consumed*, not silently dropped or
//!    defaulted), and
//! 3. every field the compiler emits is preserved through `from_json` (parse drops nothing) —
//!    caught by a non-null-leaf round-trip superset check.

use std::{collections::BTreeSet, fs, process::Command};

use fraiseql_core::{runtime::RuntimeConfig, schema::CompiledSchema};
use tempfile::TempDir;

/// Compile `fraiseql.toml` + `types.json` with the real CLI binary, returning the
/// compiled JSON string. Panics with the CLI's stderr on failure.
fn compile(types_json: &str, toml_config: &str) -> String {
    let temp_dir = TempDir::new().unwrap();
    let types_path = temp_dir.path().join("types.json");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let output_path = temp_dir.path().join("schema.compiled.json");
    fs::write(&types_path, types_json).unwrap();
    fs::write(&toml_path, toml_config).unwrap();

    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
    let output = Command::new(cli_path)
        .args([
            "compile",
            toml_path.to_str().unwrap(),
            "--types",
            types_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run fraiseql-cli");
    assert!(
        output.status.success(),
        "CLI compile failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(&output_path).expect("compiled schema missing")
}

const TYPES_JSON: &str = r#"
{
  "types": [
    {
      "name": "Order",
      "sql_source": "v_order",
      "fields": [
        {"name": "id",     "type": "ID",     "nullable": false},
        {"name": "amount", "type": "Float",  "nullable": false}
      ]
    }
  ],
  "queries": [
    {
      "name": "orders",
      "return_type": "Order",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_order",
      "cache_ttl_seconds": 300,
      "inject": {"tenant_id": "jwt:tenant_id"}
    }
  ],
  "mutations": [
    {
      "name": "createOrder",
      "return_type": "Order",
      "sql_source": "fn_create_order",
      "invalidates_views": ["v_order"]
    }
  ]
}
"#;

/// The boot seam (`RuntimeConfig::from_compiled_schema`) must accept the compiler's
/// output, AND an enterprise toggle set in TOML must survive emit → parse → derive.
/// This is the exact chain the `token_revocation: null` / audit-default incidents
/// broke: the compiler emitted a config the server then refused or silently dropped.
#[test]
fn enterprise_audit_toggle_survives_emit_parse_runtime() {
    let toml = r#"
[schema]
name = "contract_audit"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]
default_policy = "public"

[security.enterprise]
rate_limiting_enabled = false
audit_logging_enabled = true
"#;
    let compiled_json = compile(TYPES_JSON, toml);
    let schema = CompiledSchema::from_json(&compiled_json, false)
        .expect("core must parse CLI-produced schema");

    // The boot seam must not refuse valid compiler output.
    let runtime = RuntimeConfig::from_compiled_schema(&schema)
        .expect("RuntimeConfig::from_compiled_schema must accept compiler output");

    // `audit_logging_enabled = true` in TOML must reach the server's runtime config.
    assert!(
        runtime.audit_mutations,
        "audit_logging_enabled=true must survive emit→parse→runtime (it was consumed)"
    );
}

/// With the toggle off, the derived runtime config must reflect that (proving the
/// value is read, not hardcoded). Guards against a regression where the seam ignores
/// the compiled value entirely.
#[test]
fn enterprise_audit_toggle_off_is_respected() {
    let toml = r#"
[schema]
name = "contract_audit_off"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]
default_policy = "public"

[security.enterprise]
rate_limiting_enabled = false
audit_logging_enabled = false
"#;
    let compiled_json = compile(TYPES_JSON, toml);
    let schema = CompiledSchema::from_json(&compiled_json, false).unwrap();
    let runtime = RuntimeConfig::from_compiled_schema(&schema).unwrap();
    assert!(!runtime.audit_mutations, "audit_logging_enabled=false must be respected");
}

/// Every field the compiler emits must be consumed by the core parse: parse the
/// compiled JSON, re-serialize the parsed model, and assert no non-null field present
/// in the compiler output is missing from the re-serialized model. A field the parse
/// model doesn't know is silently dropped by serde — this check fails when that
/// happens. (`_content_hash` is a CLI-added envelope field, not a schema field.)
#[test]
fn every_emitted_field_survives_parse_roundtrip() {
    let toml = r#"
[schema]
name = "contract_roundtrip"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[queries.getOrder]
return_type = "Order"
return_array = false
sql_source = "v_order"

[[queries.getOrder.args]]
name = "orderId"
type = "ID"
required = true

[security]
default_policy = "public"

[[security.rules]]
name = "read_own"
rule = "user.id == object.owner_id"
description = "owner read"
cacheable = true
cache_ttl_seconds = 300

[[security.policies]]
name = "admin_only"
type = "rbac"
roles = ["admin"]
strategy = "any"
description = "admins"
cache_ttl_seconds = 600

[security.enterprise]
rate_limiting_enabled = true
audit_logging_enabled = true
"#;
    let compiled_json = compile(TYPES_JSON, toml);

    let emitted: serde_json::Value = serde_json::from_str(&compiled_json).unwrap();
    let schema = CompiledSchema::from_json(&compiled_json, false).unwrap();
    let reparsed = serde_json::to_value(&schema).unwrap();

    let mut emitted_paths = BTreeSet::new();
    collect_nonnull_paths(&emitted, "", &mut emitted_paths);
    let mut reparsed_paths = BTreeSet::new();
    collect_nonnull_paths(&reparsed, "", &mut reparsed_paths);

    // `_content_hash` is the CLI's integrity envelope, not a CompiledSchema field.
    let dropped: Vec<&String> = emitted_paths
        .iter()
        .filter(|p| !p.starts_with("/_content_hash"))
        .filter(|p| !reparsed_paths.contains(*p))
        .collect();

    assert!(
        dropped.is_empty(),
        "core parse silently dropped {} compiler-emitted field(s); the emit↔parse \
         contract has drifted (a field added to the compiler is not consumed by \
         CompiledSchema). Dropped paths:\n  {}",
        dropped.len(),
        dropped.iter().map(|p| p.as_str()).collect::<Vec<_>>().join("\n  ")
    );
}

/// Collect the set of JSON paths whose value is a non-null scalar or a container,
/// so a field present in one document but absent in the other is detectable.
fn collect_nonnull_paths(value: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let path = format!("{prefix}/{k}");
                if !v.is_null() {
                    out.insert(path.clone());
                }
                collect_nonnull_paths(v, &path, out);
            }
        },
        serde_json::Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                let path = format!("{prefix}/{i}");
                if !v.is_null() {
                    out.insert(path.clone());
                }
                collect_nonnull_paths(v, &path, out);
            }
        },
        _ => {},
    }
}
