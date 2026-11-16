#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! `[caching]` in the `fraiseql.toml` beside an SDK `schema.json` (#1719), through the
//! real binary.
//!
//! `[[caching.rules]]` is not a section of the compiled schema: each rule lowers onto the
//! query it names (`cache_ttl_seconds`) and onto its trigger mutations
//! (`invalidates_views`). Beside a schema document the table was refused as an unknown
//! field, so an SDK author could only cache a query by declaring the TTL in the SDK. It
//! lowers there as it does in a TOML schema, by the same function, with the same refusals.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

/// The schema an SDK emits: a list query and a mutation over one type.
fn document() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": [{"name": "createOrder", "return_type": "Order",
                       "sql_source": "fn_create_order", "operation": "insert",
                       "arguments": []}]
    })
}

const RULE: &str = "[caching]\nenabled = true\n\n[[caching.rules]]\nquery = \"orders\"\n\
                    ttl_seconds = 120\ninvalidation_triggers = [\"createOrder\"]\n";

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

/// `toml` as the project config beside `document`.
fn beside(toml: &str, document: &Value) -> Compiled {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), document.to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), format!("{toml}\n[project]\nname = \"app\"\n"))
        .unwrap();
    run(&dir, &["schema.json"])
}

/// `toml` as a TOML schema whose types document is `document`.
fn as_a_toml_schema(toml: &str, document: &Value) -> Compiled {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), document.to_string()).unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        format!("{toml}\n[schema]\nname = \"app\"\nversion = \"1.0.0\"\n"),
    )
    .unwrap();
    run(&dir, &["fraiseql.toml", "--types", "types.json"])
}

fn operation<'a>(schema: &'a Value, kind: &str, name: &str) -> &'a Value {
    schema[kind]
        .as_array()
        .and_then(|all| all.iter().find(|one| one["name"] == name))
        .unwrap_or_else(|| panic!("no `{name}` in {kind}: {schema}"))
}

#[test]
fn a_caching_rule_beside_a_schema_document_lowers_as_in_a_toml_schema() {
    let reference = as_a_toml_schema(RULE, &document());
    assert!(reference.ok, "the TOML workflow: {}", reference.log);

    let compiled = beside(RULE, &document());

    assert!(compiled.ok, "beside a schema document: {}", compiled.log);
    let query = operation(&compiled.schema, "queries", "orders");
    let mutation = operation(&compiled.schema, "mutations", "createOrder");
    assert_eq!(query["cache_ttl_seconds"], json!(120), "{query}");
    assert_eq!(mutation["invalidates_views"], json!(["v_order"]), "{mutation}");
    assert_eq!(query, operation(&reference.schema, "queries", "orders"));
    assert_eq!(mutation, operation(&reference.schema, "mutations", "createOrder"));
}

/// Without the table the query is not cached: the rule is what sets the TTL.
#[test]
fn without_a_rule_the_query_declares_no_ttl() {
    let compiled = beside("", &document());

    assert!(compiled.ok, "{}", compiled.log);
    assert!(operation(&compiled.schema, "queries", "orders")["cache_ttl_seconds"].is_null());
}

/// A TTL the document already declares and a rule for the same query are two authors of one
/// setting: refused, as in a TOML schema, never last-write-wins.
#[test]
fn a_rule_for_a_query_whose_document_declares_a_ttl_is_refused() {
    let mut with_ttl = document();
    with_ttl["queries"][0]["cache_ttl_seconds"] = json!(30);

    let refused = beside(RULE, &with_ttl);

    assert!(!refused.ok, "the rule replaced the document's TTL");
    assert!(
        refused.log.contains("orders") && refused.log.contains("already declares"),
        "{}",
        refused.log
    );
}

/// Each configuration the TOML workflow refuses is refused here, for the same reason.
#[test]
fn a_caching_table_is_refused_for_what_the_toml_workflow_refuses() {
    let rule = |query: &str, trigger: &str| {
        format!(
            "[caching]\nenabled = true\n\n[[caching.rules]]\nquery = \"{query}\"\n\
             ttl_seconds = 60\ninvalidation_triggers = [\"{trigger}\"]\n"
        )
    };
    for (toml, needle) in [
        (rule("ordres", "createOrder"), "does not exist"),
        (rule("orders", "makeOrder"), "not a declared mutation"),
        (RULE.replace("enabled = true", "enabled = false"), "enabled = false"),
        ("[caching]\nenabled = true\n".to_string(), "no [[caching.rules]]"),
        ("[caching]\nbackend = \"redis\"\n".to_string(), "only \"memory\" exists"),
    ] {
        let reference = as_a_toml_schema(&toml, &document());
        assert!(
            !reference.ok && reference.log.contains(needle),
            "TOML workflow: {}",
            reference.log
        );

        let refused = beside(&toml, &document());

        assert!(!refused.ok, "compiled beside a schema document: {toml}");
        assert!(refused.log.contains(needle), "{toml}: {}", refused.log);
    }
}
