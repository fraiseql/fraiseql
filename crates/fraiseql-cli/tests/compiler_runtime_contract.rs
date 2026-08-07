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
      "inject_params": {"tenant_id": "jwt:tenant_id"}
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

# Note: [[security.rules]] / [[security.policies]] / [security] default_policy are
# intentionally absent — declared-but-unenforced authorization is rejected
# (#612 item 4 / #626 / #983). The emit↔parse contract this test guards is exercised
# by the remaining security fields (enterprise).

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

/// #379: `[security] persisted_queries_only = true` must survive emit → parse into
/// the **typed** `SecurityConfig::persisted_queries_only` field the server reads in
/// `trusted_docs_from_schema` (#977) — a rename on either side is now a compile or
/// load error, never a silently-disabled flag.
#[test]
fn persisted_queries_only_survives_emit_parse() {
    let toml = r#"
[schema]
name = "contract_pqo"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]
persisted_queries_only = true
"#;
    let compiled_json = compile(TYPES_JSON, toml);
    let schema = CompiledSchema::from_json(&compiled_json, false)
        .expect("core must parse CLI-produced schema");
    let security = schema.security.expect("security must be present");
    assert!(
        security.persisted_queries_only,
        "persisted_queries_only=true must survive emit→parse into the typed field"
    );
}

/// Omitting the flag must compile to `persisted_queries_only = false` — never
/// silently strict — so a schema without the flag leaves the server permissive.
#[test]
fn persisted_queries_only_defaults_false() {
    let toml = r#"
[schema]
name = "contract_pqo_off"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]
"#;
    let compiled_json = compile(TYPES_JSON, toml);
    let schema = CompiledSchema::from_json(&compiled_json, false).unwrap();
    let security = schema.security.expect("security must be present");
    assert!(
        !security.persisted_queries_only,
        "omitting the flag must compile to false, not strict-by-default"
    );
}

/// #379: `[security.cost_budget]` must survive emit → parse into the **typed**
/// `SecurityConfig::cost_budget` field the executor and tenant registry read —
/// not into the untyped `additional` map, where a key rename would silently
/// disable the ceiling.
#[test]
fn cost_budget_survives_emit_parse_into_typed_field() {
    let toml = r#"
[schema]
name = "contract_cost_budget"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]

[security.cost_budget]
per_request_max = 50
per_tenant_per_minute_default = 1000
"#;
    let compiled_json = compile(TYPES_JSON, toml);
    let schema = CompiledSchema::from_json(&compiled_json, false)
        .expect("core must parse CLI-produced schema");
    let security = schema.security.expect("security must be present");
    let budget = security.cost_budget.expect("cost_budget must land in the typed field");
    assert_eq!(budget.per_request_max, Some(50));
    assert_eq!(budget.per_tenant_per_minute_default, Some(1000));
    // The untyped `additional` map is gone (#977): `SecurityConfig` denies
    // unknown fields, so "falls through to a catch-all" is unrepresentable.
}

/// Omitting `[security.cost_budget]` must compile to no budget — never a
/// surprise default ceiling.
#[test]
fn cost_budget_defaults_to_absent() {
    let toml = r#"
[schema]
name = "contract_cost_budget_off"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]
"#;
    let compiled_json = compile(TYPES_JSON, toml);
    let schema = CompiledSchema::from_json(&compiled_json, false).unwrap();
    let security = schema.security.expect("security must be present");
    assert!(
        security.cost_budget.is_none(),
        "omitting [security.cost_budget] must mean no budget, got {:?}",
        security.cost_budget
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

// ── #623: [[caching.rules]] lower onto the compiled per-query TTL and
//    per-mutation invalidates_views — the fields the runtime already consumes ──

/// Fixture with a TTL-less query, a second query, and a mutation, so the rules
/// (not SDK annotations) are what the assertions observe.
const CACHING_TYPES_JSON: &str = r#"
{
  "types": [
    {
      "name": "Order",
      "sql_source": "v_order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false}
      ]
    },
    {
      "name": "Inventory",
      "sql_source": "v_inventory",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false}
      ]
    }
  ],
  "queries": [
    {
      "name": "orders",
      "return_type": "Order",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_order"
    },
    {
      "name": "inventory",
      "return_type": "Inventory",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_inventory"
    }
  ],
  "mutations": [
    {
      "name": "createOrder",
      "return_type": "Order",
      "sql_source": "fn_create_order"
    }
  ]
}
"#;

const CACHING_TOML_HEAD: &str = r#"
[schema]
name = "contract_caching"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"
"#;

/// A `[[caching.rules]]` entry lowers its TTL onto the named query's
/// `cache_ttl_seconds` and appends the query's view to each trigger mutation's
/// `invalidates_views` — the two compiled fields the P12-tested runtime
/// (per-view TTL map + mutation-driven invalidation) actually consumes.
#[test]
fn caching_rule_lowers_ttl_and_invalidation_triggers() {
    let toml = format!(
        r#"{CACHING_TOML_HEAD}
[caching]
enabled = true

[[caching.rules]]
query = "inventory"
ttl_seconds = 120
invalidation_triggers = ["createOrder"]
"#
    );
    let compiled_json = compile(CACHING_TYPES_JSON, &toml);
    let schema = CompiledSchema::from_json(&compiled_json, false)
        .expect("core must parse CLI-produced schema");

    let query = schema
        .queries
        .iter()
        .find(|q| q.name == "inventory")
        .expect("inventory query present");
    assert_eq!(query.cache_ttl_seconds, Some(120), "rule TTL must land on the query");

    let mutation = schema
        .mutations
        .iter()
        .find(|m| m.name == "createOrder")
        .expect("createOrder mutation present");
    assert!(
        mutation.invalidates_views.iter().any(|v| v == "v_inventory"),
        "the trigger mutation must gain the rule query's view in invalidates_views, got {:?}",
        mutation.invalidates_views
    );
}

/// A rule naming a query that does not exist must fail the compile loudly —
/// a cache rule that silently matches nothing is the #612 config-honesty bug
/// this section was rejected over.
#[test]
fn caching_rule_with_unknown_query_fails_compile() {
    let toml = format!(
        r#"{CACHING_TOML_HEAD}
[caching]
enabled = true

[[caching.rules]]
query = "no_such_query"
ttl_seconds = 60
invalidation_triggers = []
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &toml);
    assert!(err.contains("no_such_query"), "the error must name the unknown query: {err}");
}

/// A trigger naming a mutation that does not exist must fail the compile.
#[test]
fn caching_rule_with_unknown_trigger_fails_compile() {
    let toml = format!(
        r#"{CACHING_TOML_HEAD}
[caching]
enabled = true

[[caching.rules]]
query = "inventory"
ttl_seconds = 60
invalidation_triggers = ["no_such_mutation"]
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &toml);
    assert!(
        err.contains("no_such_mutation"),
        "the error must name the unknown trigger mutation: {err}"
    );
}

/// A rule targeting a query that already carries an SDK-authored
/// `cache_ttl_seconds` must fail: two authoring sources for the same TTL must
/// not silently last-write-win.
#[test]
fn caching_rule_conflicting_with_sdk_ttl_fails_compile() {
    // TYPES_JSON's `orders` already declares cache_ttl_seconds = 300.
    let toml = format!(
        r#"{CACHING_TOML_HEAD}
[caching]
enabled = true

[[caching.rules]]
query = "orders"
ttl_seconds = 60
invalidation_triggers = []
"#
    );
    let err = compile_err(TYPES_JSON, &toml);
    assert!(
        err.contains("orders") && err.contains("cache_ttl_seconds"),
        "the error must name the doubly-declared query and field: {err}"
    );
}

/// `enabled = false` with rules, `enabled = true` with no rules, a non-memory
/// backend, and a set `redis_url` are each refused: every one is a
/// configuration that silently does nothing (or claims a backend that does
/// not exist — there is no Redis result cache anywhere in the runtime).
#[test]
fn caching_dishonest_configurations_fail_compile() {
    let disabled_with_rules = format!(
        r#"{CACHING_TOML_HEAD}
[caching]
enabled = false

[[caching.rules]]
query = "inventory"
ttl_seconds = 60
invalidation_triggers = []
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &disabled_with_rules);
    assert!(err.contains("enabled"), "disabled-with-rules must be refused: {err}");

    let enabled_without_rules = format!(
        r"{CACHING_TOML_HEAD}
[caching]
enabled = true
"
    );
    let err = compile_err(CACHING_TYPES_JSON, &enabled_without_rules);
    assert!(
        err.contains("rules"),
        "enabled-without-rules does nothing and must be refused: {err}"
    );

    let redis_backend = format!(
        r#"{CACHING_TOML_HEAD}
[caching]
enabled = true
backend = "redis"

[[caching.rules]]
query = "inventory"
ttl_seconds = 60
invalidation_triggers = []
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &redis_backend);
    assert!(
        err.contains("redis"),
        "a backend with no runtime counterpart must be refused: {err}"
    );
}

/// Compile expecting failure; returns combined stdout+stderr.
fn compile_err(types_json: &str, toml_config: &str) -> String {
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
        !output.status.success(),
        "compile must fail for this configuration; stdout: {} stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// ── #624: [[analytics.queries]] lower onto plain compiled QueryDefinitions —
//    operator-authored sql_source, compiled SELECT list, no new runtime path ──

/// An `[[analytics.queries]]` entry becomes a served, view-backed query:
/// compile-validated `sql_source`, `returns_list = true`, SELECT list from the
/// compiled return type. No client-supplied identifier can reach FROM or the
/// SELECT list because neither exists on this path at request time.
#[test]
fn analytics_query_lowers_to_a_served_query() {
    let toml = format!(
        r#"{CACHING_TOML_HEAD}
[analytics]
enabled = true

[[analytics.queries]]
name = "daily_orders"
return_type = "Order"
sql_source = "v_daily_orders"
description = "orders rolled up by day"
"#
    );
    let compiled_json = compile(CACHING_TYPES_JSON, &toml);
    let schema = CompiledSchema::from_json(&compiled_json, false)
        .expect("core must parse CLI-produced schema");
    let query = schema
        .queries
        .iter()
        .find(|q| q.name == "daily_orders")
        .expect("analytics query must be compiled as a query");
    assert_eq!(query.sql_source.as_deref(), Some("v_daily_orders"));
    assert_eq!(query.return_type, "Order");
    assert!(query.returns_list, "analytics queries return row sets");
}

/// A return type that is not a declared type is a compile error — the SELECT
/// list comes from the type, so an unknown type has no servable projection.
#[test]
fn analytics_query_with_unknown_return_type_fails_compile() {
    let toml = format!(
        r#"{CACHING_TOML_HEAD}
[analytics]
enabled = true

[[analytics.queries]]
name = "daily_orders"
return_type = "NoSuchType"
sql_source = "v_daily_orders"
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &toml);
    assert!(err.contains("NoSuchType"), "must name the unknown type: {err}");
}

/// A name colliding with an existing query, or ending in the `_aggregate` /
/// `_window` suffixes the executor's classifier hijacks before query
/// resolution, would be silently unreachable or ambiguous — compile error.
#[test]
fn analytics_query_reserved_or_colliding_names_fail_compile() {
    let colliding = format!(
        r#"{CACHING_TOML_HEAD}
[analytics]
enabled = true

[[analytics.queries]]
name = "orders"
return_type = "Order"
sql_source = "v_daily_orders"
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &colliding);
    assert!(err.contains("orders"), "must name the colliding query: {err}");

    let reserved = format!(
        r#"{CACHING_TOML_HEAD}
[analytics]
enabled = true

[[analytics.queries]]
name = "sales_aggregate"
return_type = "Order"
sql_source = "v_sales"
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &reserved);
    assert!(err.contains("_aggregate"), "must explain the reserved suffix: {err}");
}

/// An `sql_source` that is not a valid SQL identifier is refused by the same
/// compile-time validator every query goes through (the P01 posture: reject,
/// never escape).
#[test]
fn analytics_query_with_injection_shaped_sql_source_fails_compile() {
    let toml = format!(
        r#"{CACHING_TOML_HEAD}
[analytics]
enabled = true

[[analytics.queries]]
name = "daily_orders"
return_type = "Order"
sql_source = "v_daily; DROP TABLE tb_user --"
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &toml);
    assert!(
        err.to_lowercase().contains("identifier") || err.contains("sql_source"),
        "must be refused as an invalid identifier: {err}"
    );
}

/// `enabled = false` with queries, and `enabled = true` without queries, are
/// each refused — the same no-silent-no-op rule as [caching].
#[test]
fn analytics_dishonest_configurations_fail_compile() {
    let disabled_with_queries = format!(
        r#"{CACHING_TOML_HEAD}
[analytics]
enabled = false

[[analytics.queries]]
name = "daily_orders"
return_type = "Order"
sql_source = "v_daily_orders"
"#
    );
    let err = compile_err(CACHING_TYPES_JSON, &disabled_with_queries);
    assert!(err.contains("enabled"), "disabled-with-queries must be refused: {err}");

    let enabled_without_queries = format!(
        r"{CACHING_TOML_HEAD}
[analytics]
enabled = true
"
    );
    let err = compile_err(CACHING_TYPES_JSON, &enabled_without_queries);
    assert!(
        err.contains("queries"),
        "enabled-without-queries does nothing and must be refused: {err}"
    );
}
