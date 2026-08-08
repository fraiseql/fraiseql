//! #890: a query's list-ness must never be decided by which authoring surface the author
//! read the documentation of.
//!
//! One concept, two spellings:
//!
//! | Surface | Key |
//! |---|---|
//! | `[queries.*]` in a TOML schema | `return_array` |
//! | `schema.json` / a domain `types.json` | `returns_list` |
//!
//! `IntermediateQuery` was `#[serde(default)]` with no `deny_unknown_fields`, so an author
//! who wrote `return_array` in a JSON schema got `returns_list: false` — a compiled query
//! advertising a **single nullable object** where a list was declared — under a
//! `✓ Schema compiled successfully`. All three shipped `[domain_discovery]` examples had it.
//!
//! **What is actually being tested here.** The silent drop itself is closed structurally:
//! `deny_unknown_fields` on `IntermediateQuery` (#755) turns the unknown key into a hard
//! error on both compile workflows. What that leaves is a serde message listing all
//! seventeen valid field names, which never says the one thing the author needs to hear —
//! that the other authoring surface spells this key differently. So these tests pin two
//! separate guarantees:
//!
//! 1. **The cardinality guarantee** — no input spelled `return_array` ever yields a compiled
//!    artifact, let alone one with `returns_list: false`. This is the guarantee that was violated;
//!    it is asserted against the compiled file, not against an error.
//! 2. **The diagnostic** — the refusal names the other surface. Asserted on the *distinctive* half
//!    of the message: serde's own error already contains the word `returns_list`, so asserting only
//!    that would pass with the guard entry deleted (#909's lesson).
//!
//! Every case starts from **bytes on disk and the real binary** — not a constructed
//! `IntermediateQuery`, which is the half that was never broken. Both compile workflows are
//! covered because they deserialize at two different call sites (`commands::compile` for
//! JSON, `schema::merger` for TOML/`--schema-dir`), and guarding one is this seam's
//! recurring defect.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::TempDir;

/// Run the real `fraiseql-cli compile` in `dir` and return the raw result.
fn compile_in(dir: &Path, input: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["compile", input, "--output", "schema.compiled.json"])
        .current_dir(dir)
        .output()
        .expect("run fraiseql-cli")
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A one-type, one-query JSON schema whose query carries `query_extra` verbatim.
fn json_schema(query_extra: &str) -> String {
    format!(
        r#"{{
  "types": [{{"name": "Tenant", "sql_source": "v_tenant", "fields": [
    {{"name": "id", "type": "ID", "nullable": false}},
    {{"name": "name", "type": "String", "nullable": false}}
  ]}}],
  "queries": [{{"name": "listTenants", "return_type": "Tenant", "nullable": false,
                "sql_source": "v_tenant", "arguments": [], {query_extra}}}]
}}"#
    )
}

/// The distinctive half of the guard's message — the sentence serde cannot produce.
const NAMES_THE_OTHER_SURFACE: &str = "TOML `[queries.*]` spelling";

// ── The cardinality guarantee ─────────────────────────────────────────────────

/// The defect itself: `return_array: true` must never become a single-object query.
///
/// Asserted against the **compiled artifact**, because that is where the damage was. A
/// refusal satisfies it (nothing is compiled); a compile that produced
/// `returns_list: false` does not, and neither would one that quietly produced a list —
/// the author's file would still be wrong on the other surface.
#[test]
fn return_array_never_compiles_to_a_single_object_query() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), json_schema(r#""return_array": true"#)).unwrap();

    let out = compile_in(dir.path(), "schema.json");

    assert!(
        !out.status.success(),
        "#890: `return_array` must not compile at all — it compiled, and the query it \
         produced advertises whatever `returns_list` defaulted to.\nstdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        !dir.path().join("schema.compiled.json").exists(),
        "#890: a refused compile must leave no compiled artifact behind"
    );
}

/// Positive control for the case above: the canonical spelling still yields a list.
///
/// Without this, a guard that refused *every* query would pass the test above.
#[test]
fn the_canonical_returns_list_spelling_compiles_to_a_list() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), json_schema(r#""returns_list": true"#)).unwrap();

    let out = compile_in(dir.path(), "schema.json");
    assert!(out.status.success(), "compile failed: {}", stderr_of(&out));

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let compiled: serde_json::Value = serde_json::from_str(&compiled).unwrap();
    let query = compiled["queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["name"] == "listTenants")
        .expect("the query survives the compile");

    assert_eq!(
        query["returns_list"], true,
        "#890: a declared list must compile as a list — this is the value that was silently \
         `false`"
    );
}

// ── The diagnostic, on both compile workflows ─────────────────────────────────

/// The JSON workflow (`commands::compile`).
#[test]
fn the_json_workflow_names_the_other_surfaces_spelling() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), json_schema(r#""return_array": true"#)).unwrap();

    let err = stderr_of(&compile_in(dir.path(), "schema.json"));

    assert!(err.contains("listTenants"), "must name the offending query: {err}");
    assert!(err.contains("return_array"), "must name the key found: {err}");
    assert!(err.contains("returns_list"), "must name the key to write: {err}");
    assert!(
        err.contains(NAMES_THE_OTHER_SURFACE),
        "#890: the refusal must explain that this is the *other* authoring surface's \
         spelling. Serde's `deny_unknown_fields` message already lists `returns_list` among \
         seventeen field names, so that word alone proves nothing about this guard: {err}"
    );
}

/// The TOML / `[domain_discovery]` workflow (`schema::merger`) — a separate deserialization
/// call site, and the one the three shipped examples went through.
#[test]
fn the_domain_discovery_workflow_names_the_other_surfaces_spelling() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("schema/tenants")).unwrap();
    fs::write(
        dir.path().join("schema/tenants/types.json"),
        json_schema(r#""return_array": true"#),
    )
    .unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        "[schema]\nname = \"tenants\"\n\n[domain_discovery]\nenabled = true\nroot_dir = \"schema\"\n",
    )
    .unwrap();

    let out = compile_in(dir.path(), "fraiseql.toml");
    let err = stderr_of(&out);

    assert!(!out.status.success(), "#890: the domain-discovery path must refuse it too");
    assert!(err.contains("listTenants"), "must name the offending query: {err}");
    assert!(
        err.contains(NAMES_THE_OTHER_SURFACE),
        "#890: guarding only the JSON workflow is this seam's recurring defect — every \
         `--schema-dir` and `types.json` author would keep the bare serde message: {err}"
    );
}

// ── The other surface must keep working ───────────────────────────────────────

/// `return_array` is **correct** in `[queries.*]`, and the guard runs on the merged value
/// that path produces. If the merger ever stopped translating the key — or the guard were
/// widened to a blanket string match — every TOML-authored list query would break with a
/// message telling the author to write the key they already wrote.
#[test]
fn the_toml_queries_surface_keeps_its_own_spelling() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"[schema]
name = "tenants"
version = "1.0.0"
database_target = "postgresql"

[types.Tenant]
sql_source = "v_tenant"

[types.Tenant.fields.id]
type = "ID"

[queries.listTenants]
return_type = "Tenant"
return_array = true
sql_source = "v_tenant"
"#,
    )
    .unwrap();

    let out = compile_in(dir.path(), "fraiseql.toml");
    assert!(
        out.status.success(),
        "#890: `return_array` is the correct key on this surface and must keep compiling: {}",
        stderr_of(&out)
    );

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let compiled: serde_json::Value = serde_json::from_str(&compiled).unwrap();
    let query = compiled["queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["name"] == "listTenants")
        .expect("the query survives the compile");

    assert_eq!(
        query["returns_list"], true,
        "#890: the TOML surface's `return_array` must still lower onto `returns_list`"
    );
}
