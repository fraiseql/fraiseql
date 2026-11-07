//! A multi-tenant schema declares how every root it serves is scoped to the caller's tenant.
//!
//! Row isolation (`[tenancy] mode = "row"`) scopes a root by the `@tenant_id` field of
//! the type it returns. A type with no such field is either read the same by every tenant,
//! which `[tenancy] shared_types` says, or it is a per-tenant type its author forgot
//! to annotate, which the compiler cannot tell apart: so it refuses the schema until one of the
//! two is declared. The roots concerned are every query, mutation and subscription return type
//! (through union members and interface implementors).
//!
//! A fact table in a multi-tenant schema declares the same thing for its `_aggregate` and
//! `_window` roots: `inject_params`, `row_scope = "relation"`, or `row_scope = "shared"`. A
//! relation that filters its own rows needs a per-request value the server sets from the
//! caller's identity to filter on.
//!
//! **Execution engine:** the `fraiseql-cli` binary, in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** each case compiles in its own temp dir.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{fmt::Write as _, fs, process::Command};

use fraiseql_core::schema::CompiledSchema;
use serde_json::{Value, json};
use tempfile::TempDir;

/// Run `fraiseql-cli compile` in `dir` on `input`; the compiled schema, or the combined output.
fn run_compile(dir: &TempDir, input: &str, extra: &[&str]) -> Result<CompiledSchema, String> {
    let out = dir.path().join("schema.compiled.json");
    let mut argv = vec!["compile", input];
    argv.extend_from_slice(extra);
    argv.extend_from_slice(&["--output", out.to_str().unwrap()]);
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(&argv)
        .current_dir(dir.path())
        .output()
        .expect("run fraiseql-cli");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    if !result.status.success() {
        return Err(output);
    }
    let json = fs::read_to_string(&out).expect("read compiled schema");
    Ok(CompiledSchema::from_json(&json, false).expect("the compiled schema loads"))
}

/// Compile `schema` (a `schema.json` document) under the project `fraiseql.toml` body
/// `fraiseql_toml`.
fn compile_project(schema: &Value, fraiseql_toml: &str) -> Result<CompiledSchema, String> {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), format!("{fraiseql_toml}\n")).unwrap();
    run_compile(&dir, "schema.json", &[])
}

/// `[tenancy] mode = "row"`, with `shared_types` when it is not empty.
fn row_mode(shared_types: &[&str]) -> String {
    let mut toml = "[tenancy]\nmode = \"row\"\ntenant_claim = \"tenant_id\"\n".to_string();
    if !shared_types.is_empty() {
        let list: Vec<String> = shared_types.iter().map(|t| format!("\"{t}\"")).collect();
        writeln!(toml, "shared_types = [{}]", list.join(", ")).unwrap();
    }
    toml
}

/// An object type reading `view`; its `tenant_id` field carries `@tenant_id` when `annotated`.
fn object_type(name: &str, view: &str, annotated: bool) -> Value {
    let mut tenant = json!({"name": "tenant_id", "type": "String", "nullable": false});
    if annotated {
        tenant["directives"] = json!([{"name": "tenant_id"}]);
    }
    json!({
        "name": name,
        "sql_source": view,
        "fields": [{"name": "id", "type": "ID", "nullable": false}, tenant]
    })
}

/// `Account` is annotated and listed by `accounts`; `Note` is not annotated, and has no
/// relation of its own, so only the roots a case adds reach it.
fn base_schema() -> Value {
    let mut note = object_type("Note", "v_note", false);
    note.as_object_mut().unwrap().remove("sql_source");
    json!({
        "types": [object_type("Account", "v_account", true), note],
        "queries": [
            {"name": "accounts", "return_type": "Account", "returns_list": true, "sql_source": "v_account"}
        ],
        "mutations": [],
        "subscriptions": []
    })
}

fn with_query_returning(mut schema: Value, return_type: &str) -> Value {
    schema["queries"].as_array_mut().unwrap().push(json!({
        "name": "notes", "return_type": return_type, "returns_list": true, "sql_source": "v_note"
    }));
    schema
}

/// Assert `result` is a refusal naming `type_name` and the two ways to declare it.
fn assert_refused_naming(result: Result<CompiledSchema, String>, type_name: &str, label: &str) {
    let Err(output) = result else {
        panic!("{label}: compiled, so `{type_name}` is served to every tenant unscoped")
    };
    assert!(
        output.contains(&format!("'{type_name}'"))
            && output.contains("@tenant_id")
            && output.contains("shared_types"),
        "{label}: refused, but not for the undeclared type `{type_name}`: {output}"
    );
}

// ── Row isolation: every root's type is annotated or shared ─────────────────────

#[test]
fn row_isolation_refuses_a_queried_type_that_is_neither_annotated_nor_shared() {
    let schema = with_query_returning(base_schema(), "Note");
    assert_refused_naming(compile_project(&schema, &row_mode(&[])), "Note", "query");
}

#[test]
fn row_isolation_compiles_a_queried_type_listed_as_shared() {
    let schema = with_query_returning(base_schema(), "Note");
    let compiled = compile_project(&schema, &row_mode(&["Note"]));
    assert!(compiled.is_ok(), "a shared type compiles: {:?}", compiled.err());
}

#[test]
fn row_isolation_refuses_a_shared_type_that_is_also_annotated() {
    let output = compile_project(&base_schema(), &row_mode(&["Account"]))
        .expect_err("a type cannot be both per-tenant and shared");
    assert!(
        output.contains("'Account'") && output.contains("both"),
        "refused, but not for the contradiction: {output}"
    );
}

#[test]
fn row_isolation_refuses_an_undeclared_type_reached_through_a_union() {
    let mut schema = with_query_returning(base_schema(), "Feed");
    schema["unions"] = json!([{"name": "Feed", "member_types": ["Account", "Note"]}]);
    assert_refused_naming(compile_project(&schema, &row_mode(&[])), "Note", "union member");
}

#[test]
fn row_isolation_refuses_an_undeclared_type_reached_through_an_interface() {
    let mut schema = with_query_returning(base_schema(), "Owned");
    schema["interfaces"] =
        json!([{"name": "Owned", "fields": [{"name": "id", "type": "ID", "nullable": false}]}]);
    schema["types"][0]["implements"] = json!(["Owned"]);
    schema["types"][1]["implements"] = json!(["Owned"]);
    assert_refused_naming(
        compile_project(&schema, &row_mode(&[])),
        "Note",
        "interface implementor",
    );
}

#[test]
fn row_isolation_refuses_a_subscription_to_an_undeclared_type() {
    let mut schema = base_schema();
    schema["subscriptions"] =
        json!([{"name": "noteChanged", "return_type": "Note", "topic": "note"}]);
    assert_refused_naming(compile_project(&schema, &row_mode(&[])), "Note", "subscription");
}

/// `base_schema` with `Note` given a relation that no query returns, and, when `federation`
/// is set, declared a federation entity of a federation block enabled or not.
fn with_queryless_note(federation: Option<bool>) -> Value {
    let mut schema = base_schema();
    schema["types"][1]["sql_source"] = json!("v_note");
    if let Some(enabled) = federation {
        schema["federation"] = json!({
            "enabled": enabled, "service_name": "notes", "version": "v2",
            "entities": [{"name": "Note", "key_fields": ["id"]}]
        });
    }
    schema
}

/// A federation entity no query returns is resolved through `_entities`: it is a root
/// too.
#[test]
fn row_isolation_refuses_a_queryless_entity_that_is_neither_annotated_nor_shared() {
    let schema = with_queryless_note(Some(true));
    assert_refused_naming(compile_project(&schema, &row_mode(&[])), "Note", "queryless entity");
}

/// Without federation there is no `_entities` root: a type no root returns is read by
/// nothing, whatever relation it names.
#[test]
fn row_isolation_compiles_a_queryless_type_when_nothing_federates_it() {
    let compiled = compile_project(&with_queryless_note(None), &row_mode(&[]));
    assert!(compiled.is_ok(), "a type no root reads compiles: {:?}", compiled.err());
}

/// A federation block that is not enabled serves no `_entities` root either, whatever
/// entities it lists.
#[test]
fn row_isolation_compiles_a_listed_entity_when_federation_is_disabled() {
    let compiled = compile_project(&with_queryless_note(Some(false)), &row_mode(&[]));
    assert!(
        compiled.is_ok(),
        "an entity of a disabled federation compiles: {:?}",
        compiled.err()
    );
}

fn with_mutation_returning_note(inject: Option<Value>) -> Value {
    let mut schema = base_schema();
    let mut mutation = json!({
        "name": "addNote", "return_type": "Note", "sql_source": "fn_add_note", "operation": "insert"
    });
    if let Some(inject) = inject {
        mutation["inject_params"] = inject;
    }
    schema["mutations"] = json!([mutation]);
    schema
}

#[test]
fn row_isolation_refuses_a_mutation_to_an_undeclared_type_that_passes_no_tenant() {
    let schema = with_mutation_returning_note(None);
    assert_refused_naming(compile_project(&schema, &row_mode(&[])), "Note", "mutation");
}

/// A mutation whose function receives the caller's tenant is scoped by it, whatever the
/// type it returns.
#[test]
fn row_isolation_compiles_a_mutation_that_passes_the_tenant_to_its_function() {
    let schema = with_mutation_returning_note(Some(json!({"p_tenant": "jwt:tenant_id"})));
    let compiled = compile_project(&schema, &row_mode(&[]));
    assert!(compiled.is_ok(), "a tenant-bound mutation compiles: {:?}", compiled.err());
}

/// The TOML-schema workflow (`fraiseql compile schema.toml --types …`) lowers `[tenancy]`
/// through its own path: it must carry the same rule.
#[test]
fn a_toml_schema_carries_the_same_rule() {
    let types = json!({
        "types": [object_type("Account", "v_account", true), object_type("Note", "v_note", false)]
    });
    let toml_schema = |shared: &str| {
        format!(
            "[project]\nname = \"tenant_scope\"\nversion = \"1.0.0\"\n\
             database_target = \"postgresql\"\n\n\
             [queries.notes]\nreturn_type = \"Note\"\nreturn_array = true\nsql_source = \"v_note\"\n\n\
             [security]\n\n[tenancy]\nmode = \"row\"\ntenant_claim = \"tenant_id\"\n{shared}"
        )
    };
    let compile_toml = |shared: &str| {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("types.json"), types.to_string()).unwrap();
        fs::write(dir.path().join("schema.toml"), toml_schema(shared)).unwrap();
        run_compile(&dir, "schema.toml", &["--types", "types.json"])
    };
    assert_refused_naming(compile_toml(""), "Note", "TOML schema");
    let compiled = compile_toml("shared_types = [\"Note\"]\n");
    assert!(compiled.is_ok(), "TOML schema, shared: {:?}", compiled.err());
}

// ── Fact tables in a multi-tenant schema ────────────────────────────────────────

/// `tf_sales` keeps its tenant in `org_id`, a column no query injects, and links no type.
fn with_fact_table(mut schema: Value, extra: &Value) -> Value {
    let mut fact_table = json!({
        "table_name": "tf_sales",
        "measures": [{"name": "revenue", "sql_type": "numeric", "nullable": false}],
        "dimensions": {"name": "data", "paths": []},
        "denormalized_filters": [{"name": "org_id", "sql_type": "text", "indexed": true}]
    });
    for (key, value) in extra.as_object().unwrap() {
        fact_table[key] = value.clone();
    }
    schema["fact_tables"] = json!([fact_table]);
    schema
}

fn assert_fact_table_refused(result: Result<CompiledSchema, String>, label: &str) {
    let Err(output) = result else {
        panic!("{label}: compiled, so tf_sales' aggregates read every tenant's rows")
    };
    assert!(
        output.contains("tf_sales") && output.contains("row_scope = \"shared\""),
        "{label}: refused, but not for tf_sales' undeclared scope: {output}"
    );
}

#[test]
fn row_isolation_refuses_a_fact_table_that_declares_no_scope() {
    let schema = with_fact_table(base_schema(), &json!({}));
    assert_fact_table_refused(compile_project(&schema, &row_mode(&[])), "row isolation");
}

#[test]
fn a_multi_tenant_schema_refuses_a_fact_table_that_declares_no_scope() {
    let schema = with_fact_table(base_schema(), &json!({}));
    let toml = "[security]\nmulti_tenant = true\n\n[security.rls]\nenabled = true\n";
    assert_fact_table_refused(compile_project(&schema, toml), "multi_tenant = true");
}

#[test]
fn a_multi_tenant_schema_compiles_a_fact_table_declared_shared() {
    let schema = with_fact_table(base_schema(), &json!({"row_scope": "shared"}));
    let compiled = compile_project(&schema, &row_mode(&[]));
    assert!(compiled.is_ok(), "a shared fact table compiles: {:?}", compiled.err());
}

/// A single-tenant schema has no other tenant to read: its fact tables keep the compiler's
/// inference, and one nothing infers scoped compiles undeclared.
#[test]
fn a_single_tenant_schema_compiles_a_fact_table_that_declares_no_scope() {
    let mut schema = with_fact_table(base_schema(), &json!({}));
    schema["queries"][0]["inject_params"] = json!({"tenant_id": "jwt:tenant_id"});
    let compiled = compile_project(&schema, "");
    assert!(compiled.is_ok(), "single-tenant: {:?}", compiled.err());
}

// ── A relation-scoped fact table filters on a value the server sets ────────────

fn relation_scoped(session_variables: &Value) -> Value {
    let mut schema = with_fact_table(base_schema(), &json!({"row_scope": "relation"}));
    schema["session_variables"] = session_variables.clone();
    schema
}

fn assert_relation_refused(result: Result<CompiledSchema, String>, label: &str) {
    let Err(output) = result else {
        panic!("{label}: compiled, though nothing per-request scopes the relation")
    };
    assert!(
        output.contains("tf_sales")
            && output.contains("row_scope = \"relation\"")
            && output.contains("session_variables"),
        "{label}: refused, but not for the missing per-request value: {output}"
    );
}

#[test]
fn a_relation_scoped_fact_table_without_a_session_variable_is_refused() {
    let schema = relation_scoped(&json!({"variables": []}));
    assert_relation_refused(compile_project(&schema, &row_mode(&[])), "no session variable");
}

#[test]
fn a_relation_scoped_fact_table_on_a_header_is_refused() {
    let schema = relation_scoped(&json!({"variables": [
        {"name": "app.org_id", "source": "header", "header": "x-org-id"}
    ]}));
    assert_relation_refused(compile_project(&schema, &row_mode(&[])), "header-sourced");
}

#[test]
fn a_relation_scoped_fact_table_on_a_token_claim_compiles() {
    let schema = relation_scoped(&json!({"variables": [
        {"name": "app.org_id", "source": "jwt", "claim": "tenant_id"}
    ]}));
    let compiled = compile_project(&schema, &row_mode(&[]));
    assert!(compiled.is_ok(), "a claim-scoped relation compiles: {:?}", compiled.err());
}

/// An enriched-identity field is resolved by the server from the database, never from the
/// client: it is a per-request value the relation can filter on.
#[test]
fn a_relation_scoped_fact_table_on_an_enriched_field_compiles() {
    let schema = relation_scoped(&json!({"variables": [
        {"name": "app.org_id", "source": "enrichment", "field": "org_id"}
    ]}));
    let compiled = compile_project(&schema, &row_mode(&[]));
    assert!(compiled.is_ok(), "an enrichment-scoped relation compiles: {:?}", compiled.err());
}
