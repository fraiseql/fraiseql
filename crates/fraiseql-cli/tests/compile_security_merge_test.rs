#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! A schema document's `security` block beside a TOML file, through the real binary, on
//! both workflows that read one: an SDK `schema.json` compiled beside a project
//! `fraiseql.toml`, and a TOML schema compiled with `--types`.
//!
//! The merge rule: a key set by both sources must agree, or the compile is refused naming
//! the key; a key only the schema document sets is kept; a key only the TOML sets (or
//! defaults) comes from the TOML. Comparison is per top-level section, so a section the
//! TOML declares partially must still match the document's copy in full. The row-tenancy
//! `@tenant_id` check reads the merged result.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

/// The two workflows that combine a schema document with a TOML file.
#[derive(Clone, Copy, Debug)]
enum Workflow {
    /// `fraiseql compile schema.json` beside a project `fraiseql.toml`.
    Sdk,
    /// `fraiseql compile fraiseql.toml --types types.json`.
    Types,
}

const PROJECT_ONLY: &str = "[project]\nname = \"app\"\n";
const SCHEMA_ONLY: &str = "[project]\nname = \"app\"\nversion = \"1.0.0\"\n";

const fn base_toml(path: Workflow) -> &'static str {
    match path {
        Workflow::Sdk => PROJECT_ONLY,
        Workflow::Types => SCHEMA_ONLY,
    }
}

fn order_type(with_tenant_field: bool) -> Value {
    let mut fields = vec![json!({"name": "id", "type": "ID", "nullable": false})];
    if with_tenant_field {
        fields.push(json!({"name": "tenantId", "type": "ID", "nullable": false,
                           "directives": [{"name": "tenant_id"}]}));
    }
    json!({"name": "Order", "sql_source": "v_order", "fields": fields})
}

fn document(security: Option<Value>, with_tenant_field: bool) -> Value {
    let mut doc = json!({
        "types": [order_type(with_tenant_field)],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": []
    });
    if let Some(security) = security {
        doc["security"] = security;
    }
    doc
}

/// Row tenancy on `org_id`, RLS, a cost budget and `multi_tenant`.
fn tenant_security() -> Value {
    json!({
        "cost_budget": {"per_request_max": 100},
        "tenancy": {"mode": "row", "tenant_claim": "org_id"},
        "rls": {"enabled": true},
        "multi_tenant": true
    })
}

/// Lay out a project for `path`; `toml = None` writes no TOML file (SDK path only).
fn project(path: Workflow, toml: Option<&str>, doc: &Value) -> TempDir {
    let dir = TempDir::new().unwrap();
    let doc_name = match path {
        Workflow::Sdk => "schema.json",
        Workflow::Types => "types.json",
    };
    fs::write(dir.path().join(doc_name), doc.to_string()).unwrap();
    if let Some(toml) = toml {
        fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    }
    dir
}

fn run(path: Workflow, dir: &Path) -> (bool, String) {
    let out = dir.join("schema.compiled.json");
    let mut args = vec!["compile"];
    match path {
        Workflow::Sdk => args.push("schema.json"),
        Workflow::Types => args.extend_from_slice(&["fraiseql.toml", "--types", "types.json"]),
    }
    args.extend_from_slice(&["--output", out.to_str().unwrap()]);
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(&args)
        .current_dir(dir)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    (result.status.success(), log)
}

fn compiled(path: Workflow, dir: &TempDir) -> Value {
    let (ok, log) = run(path, dir.path());
    assert!(ok, "{path:?}: compile failed: {log}");
    let json = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn refused(path: Workflow, dir: &TempDir) -> String {
    let (ok, log) = run(path, dir.path());
    assert!(!ok, "{path:?}: expected the compile to be refused; log: {log}");
    log
}

fn orders_inject_params(schema: &Value) -> Value {
    schema["queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["name"] == "orders")
        .unwrap()["inject_params"]
        .clone()
}

// ── Keys only the schema document sets are kept ─────────────────────────────────────

fn document_keys_survive_a_toml_that_does_not_set_them(path: Workflow) {
    let dir = project(path, Some(base_toml(path)), &document(Some(tenant_security()), true));
    let schema = compiled(path, &dir);
    let security = &schema["security"];
    assert_eq!(security["cost_budget"]["per_request_max"], json!(100), "{path:?}: {security}");
    assert_eq!(security["tenancy"]["mode"], json!("row"), "{path:?}: {security}");
    assert_eq!(security["tenancy"]["tenant_claim"], json!("org_id"), "{path:?}: {security}");
    assert_eq!(security["rls"]["enabled"], json!(true), "{path:?}: {security}");
    assert_eq!(security["multi_tenant"], json!(true), "{path:?}: {security}");
    assert_eq!(
        orders_inject_params(&schema),
        json!({"tenantId": {"source": "jwt", "claim": "org_id"}}),
        "{path:?}: row tenancy must scope the query by the document's claim"
    );
}

#[test]
fn sdk_document_security_keys_survive_a_project_only_toml() {
    document_keys_survive_a_toml_that_does_not_set_them(Workflow::Sdk);
}

#[test]
fn types_document_security_keys_survive_a_schema_only_toml() {
    document_keys_survive_a_toml_that_does_not_set_them(Workflow::Types);
}

/// `default_policy` has no TOML spelling and no enforcer: a compile of the document alone
/// refuses it, and so does a compile beside a TOML, because the document's key reaches
/// the merged section that gate reads.
#[test]
fn sdk_document_default_policy_reaches_the_gate_beside_a_project_only_toml() {
    let security = json!({"default_policy": "authenticated"});
    for toml in [None, Some(PROJECT_ONLY)] {
        let dir = project(Workflow::Sdk, toml, &document(Some(security.clone()), false));
        let log = refused(Workflow::Sdk, &dir);
        assert!(log.contains("`security.default_policy`"), "toml {toml:?}: {log}");
    }
}

// ── A key set by both sources must agree ────────────────────────────────────────────

fn tenancy_disagreement_is_refused(path: Workflow, toml: &str) {
    let dir = project(path, Some(toml), &document(Some(tenant_security()), true));
    let log = refused(path, &dir);
    assert!(log.contains("`tenancy`"), "{path:?}: refusal must name the key: {log}");
}

#[test]
fn sdk_tenancy_set_by_both_and_disagreeing_is_refused() {
    tenancy_disagreement_is_refused(
        Workflow::Sdk,
        "[project]\nname = \"app\"\n\n[tenancy]\nmode = \"none\"\n",
    );
}

#[test]
fn types_tenancy_set_by_both_and_disagreeing_is_refused() {
    tenancy_disagreement_is_refused(
        Workflow::Types,
        "[project]\nname = \"app\"\nversion = \"1.0.0\"\n\n[tenancy]\nmode = \"none\"\n",
    );
}

#[test]
fn sdk_tenancy_set_by_both_and_agreeing_compiles() {
    let toml =
        "[project]\nname = \"app\"\n\n[tenancy]\nmode = \"row\"\ntenant_claim = \"org_id\"\n";
    let dir = project(Workflow::Sdk, Some(toml), &document(Some(tenant_security()), true));
    let schema = compiled(Workflow::Sdk, &dir);
    assert_eq!(schema["security"]["tenancy"]["tenant_claim"], json!("org_id"), "{schema}");
    assert_eq!(
        orders_inject_params(&schema),
        json!({"tenantId": {"source": "jwt", "claim": "org_id"}})
    );
}

/// Agreement is judged on the compiled section, not the spelling: the TOML lowers
/// `[tenancy] mode = "row"` with its default claim written out, and a document
/// copy that leaves the claim to the same default agrees with it.
#[test]
fn sdk_tenancy_spelled_differently_with_the_same_meaning_compiles() {
    let toml = "[project]\nname = \"app\"\n\n[tenancy]\nmode = \"row\"\n";
    let security = json!({"tenancy": {"mode": "row"}});
    let dir = project(Workflow::Sdk, Some(toml), &document(Some(security), true));
    let schema = compiled(Workflow::Sdk, &dir);
    assert_eq!(schema["security"]["tenancy"]["tenant_claim"], json!("tenant_id"), "{schema}");
    assert_eq!(
        orders_inject_params(&schema),
        json!({"tenantId": {"source": "jwt", "claim": "tenant_id"}})
    );
}

/// The comparison is per section: a TOML that declares `[tenancy]` with only
/// `mode` still owns the whole section, so a document claim other than the TOML's
/// default claim is a disagreement, refused by section name.
#[test]
fn sdk_a_partially_declared_section_must_match_the_document_in_full() {
    let toml = "[project]\nname = \"app\"\n\n[tenancy]\nmode = \"row\"\n";
    let dir = project(Workflow::Sdk, Some(toml), &document(Some(tenant_security()), true));
    let log = refused(Workflow::Sdk, &dir);
    assert!(log.contains("`tenancy`"), "refusal must name the section: {log}");
}

#[test]
fn types_cost_budget_set_by_both_and_disagreeing_is_refused() {
    let toml = "[project]\nname = \"app\"\nversion = \"1.0.0\"\n\n\
                [security.cost_budget]\nper_request_max = 50\n";
    let dir = project(Workflow::Types, Some(toml), &document(Some(tenant_security()), true));
    let log = refused(Workflow::Types, &dir);
    assert!(log.contains("`cost_budget`"), "refusal must name the key: {log}");
}

/// `enterprise` is its own table, `[security.enterprise]`: declaring the rate
/// limiting table no longer declares it, so the document's copy is the document's.
#[test]
fn sdk_enterprise_is_not_declared_by_the_rate_limiting_table() {
    let toml = "[project]\nname = \"app\"\n\n[security.rate_limiting]\nenabled = false\n";
    let security = json!({"enterprise": {"audit_logging_enabled": false}});
    let dir = project(Workflow::Sdk, Some(toml), &document(Some(security), false));
    let schema = compiled(Workflow::Sdk, &dir);
    assert_eq!(schema["security"]["enterprise"], json!({"audit_logging_enabled": false}));
}

// ── The row-tenancy check reads the merged result ───────────────────────────────────

fn row_tenancy_without_tenant_field_is_refused(path: Workflow) {
    let dir = project(path, Some(base_toml(path)), &document(Some(tenant_security()), false));
    let log = refused(path, &dir);
    assert!(log.contains("@tenant_id validation failed"), "{path:?}: {log}");
}

#[test]
fn sdk_row_tenancy_check_reads_the_merged_security() {
    row_tenancy_without_tenant_field_is_refused(Workflow::Sdk);
}

#[test]
fn types_row_tenancy_check_reads_the_merged_security() {
    row_tenancy_without_tenant_field_is_refused(Workflow::Types);
}

// ── Without a document `security` block, output is unchanged ────────────────────────

/// No TOML at all: the document's block is the whole `security` section.
#[test]
fn sdk_without_a_toml_the_document_security_is_compiled_as_written() {
    let dir = project(Workflow::Sdk, None, &document(Some(tenant_security()), true));
    let schema = compiled(Workflow::Sdk, &dir);
    assert_eq!(schema["security"], SDK_NO_TOML_SECURITY.parse::<Value>().unwrap());
}

/// A TOML beside a document without `security`: the TOML's lowered section, as before.
#[test]
fn sdk_a_document_without_security_takes_the_project_config_section() {
    let dir = project(Workflow::Sdk, Some(PROJECT_ONLY), &document(None, false));
    let schema = compiled(Workflow::Sdk, &dir);
    assert_eq!(schema["security"], SETTINGS_ONLY_SECURITY.parse::<Value>().unwrap());
}

#[test]
fn types_a_document_without_security_takes_the_toml_section() {
    let dir = project(Workflow::Types, Some(SCHEMA_ONLY), &document(None, false));
    let schema = compiled(Workflow::Types, &dir);
    assert_eq!(schema["security"], SETTINGS_ONLY_SECURITY.parse::<Value>().unwrap());
}

// Captured from the compiler before the merge rule existed: these three paths carry no
// document `security` block next to a TOML, so the rule must leave them unchanged. The two
// TOML ones were recaptured when `enterprise` lost its nine unread keys and an unwritten
// `error_sanitization` / `rate_limiting` stopped being emitted by a project config,
// and are one since both roles of a `fraiseql.toml` lower `[security]` alike: an unwritten
// `state_encryption` is emitted by neither.
const SDK_NO_TOML_SECURITY: &str = r#"{"multi_tenant":true,"rls":{"enabled":true},"tenancy":{"mode":"row","tenant_claim":"org_id"},"cost_budget":{"per_request_max":100}}"#;
const SETTINGS_ONLY_SECURITY: &str = r#"{"enterprise":{"audit_logging_enabled":true}}"#;
