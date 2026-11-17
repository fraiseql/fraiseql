#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! One grammar for every `fraiseql.toml`, through the real binary.
//!
//! A `fraiseql.toml` is either the compile input (a TOML schema, which may author types and
//! operations) or the file read beside a schema document. Both are read by one grammar: a
//! setting has one spelling, top-level, whichever role the file plays. A spelling the
//! grammar no longer takes is refused by name, naming the one it takes instead.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

/// The schema an SDK emits: one type and its list query.
fn document() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": []
    })
}

struct Compiled {
    ok:     bool,
    log:    String,
    schema: Value,
}

fn run(dir: &TempDir, args: &[&str]) -> Compiled {
    let out = dir.path().join("schema.compiled.json");
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .arg("compile")
        .args(args)
        .args(["--output", out.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let schema = fs::read_to_string(&out)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    Compiled {
        ok: result.status.success(),
        log,
        schema,
    }
}

/// `toml`, verbatim, as the `fraiseql.toml` beside a schema document.
fn beside_a_document(toml: &str) -> Compiled {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), document().to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    run(&dir, &["schema.json"])
}

/// `toml`, verbatim, as the compile input, its types from the same document (`--types`).
fn as_the_input(toml: &str) -> Compiled {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), document().to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    run(&dir, &["fraiseql.toml", "--types", "types.json"])
}

/// Both roles a `fraiseql.toml` plays, labelled for assertion messages.
fn in_both_roles(toml: &str) -> [(&'static str, Compiled); 2] {
    [
        ("beside a document", beside_a_document(toml)),
        ("as the input", as_the_input(toml)),
    ]
}

/// `refused` failed by the named refusal of `old`, pointing at `new` (empty: nothing to
/// write instead), not by serde's unknown-field list.
fn assert_refused_by_name(role: &str, refused: &Compiled, old: &str, new: &str) {
    assert!(!refused.ok, "{role}: {old} compiled");
    assert!(refused.log.contains("is no longer accepted"), "{role}: {old}: {}", refused.log);
    assert!(
        refused.log.contains(old),
        "{role}: the refusal does not name {old}: {}",
        refused.log
    );
    assert!(refused.log.contains(new), "{role}: {old} does not name {new}: {}", refused.log);
}

// ── metadata: `[project]` ─────────────────────────────────────────────────────

#[test]
fn project_is_the_metadata_table_in_both_roles() {
    let toml = "[project]\nname = \"app\"\nversion = \"1.2.0\"\ndatabase_target = \"postgresql\"\n";
    for (role, compiled) in in_both_roles(toml) {
        assert!(compiled.ok, "{role}: {}", compiled.log);
        assert!(compiled.schema["types"].is_array(), "{role}: no artifact: {}", compiled.log);
    }
}

#[test]
fn project_database_target_is_checked_in_both_roles() {
    let toml = "[project]\nname = \"app\"\ndatabase_target = \"mysql\"\n";
    for (role, refused) in in_both_roles(toml) {
        assert!(!refused.ok, "{role}: a mysql target compiled");
        assert!(refused.log.contains("[project]"), "{role}: {}", refused.log);
        assert!(refused.log.contains("database_target"), "{role}: {}", refused.log);
    }
}

#[test]
fn the_schema_table_is_refused_naming_project() {
    let toml = "[schema]\nname = \"app\"\nversion = \"1.0.0\"\n";
    for (role, refused) in in_both_roles(toml) {
        assert_refused_by_name(role, &refused, "[schema]", "[project]");
    }
}

#[test]
fn project_description_is_refused_as_read_by_nothing() {
    let toml = "[project]\nname = \"app\"\ndescription = \"An app\"\n";
    for (role, refused) in in_both_roles(toml) {
        assert_refused_by_name(role, &refused, "[project] description", "nothing reads it");
    }
}
