//! A scoped type embedded in another type's served document does not compile, unless the
//! embedding field declares `scoped_like_parent` and the schema bears it out (#1737).
//!
//! The rule is checked where a compiled schema loads. These tests hold the compiler to it,
//! through the real binary: the declaration written in a schema document reaches the
//! compiled field, and a document without it is refused when it is compiled, not when a
//! server boots on the artifact.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{fs, process::Command};

use fraiseql_core::schema::CompiledSchema;
use serde_json::{Value, json};
use tempfile::TempDir;

/// `Order` embeds `OrderLine`, whose list query is scoped by the tenant claim. `lines` adds
/// keys to the embedding field; `order_scope` is the `inject_params` of `Order`'s query.
fn document(lines: &Value, order_scope: &Value) -> Value {
    let mut lines_field = json!({ "name": "lines", "type": "[OrderLine]", "nullable": false });
    for (key, value) in lines.as_object().unwrap() {
        lines_field[key] = value.clone();
    }
    let field = |name: &str| json!({ "name": name, "type": "ID", "nullable": false });
    json!({
        "types": [
            { "name": "Order", "sql_source": "v_order",
              "fields": [field("id"), field("tenantId"), lines_field] },
            { "name": "OrderLine", "sql_source": "v_order_line",
              "fields": [field("id"), field("tenantId")] }
        ],
        "queries": [
            { "name": "orders", "return_type": "Order", "returns_list": true,
              "sql_source": "v_order", "inject_params": order_scope },
            { "name": "orderLines", "return_type": "OrderLine", "returns_list": true,
              "sql_source": "v_order_line", "inject_params": { "tenant_id": "jwt:org_id" } }
        ],
        "mutations": []
    })
}

fn tenant() -> Value {
    json!({ "tenant_id": "jwt:org_id" })
}

/// Compile `document` as an SDK `schema.json`: whether it compiled, what the compiler said,
/// and the artifact when it wrote one.
fn compile(document: &Value) -> (bool, String, Option<CompiledSchema>) {
    let directory = TempDir::new().unwrap();
    let schema = directory.path().join("schema.json");
    let artifact = directory.path().join("schema.compiled.json");
    fs::write(&schema, document.to_string()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "compile",
            schema.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .current_dir(directory.path())
        .output()
        .expect("failed to run fraiseql-cli");
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled = fs::read_to_string(&artifact)
        .ok()
        .map(|text| CompiledSchema::from_json(&text, false).unwrap());
    (output.status.success(), said, compiled)
}

#[test]
fn an_undeclared_scoped_embedding_does_not_compile() {
    let (compiled, said, artifact) = compile(&document(&json!({}), &tenant()));

    assert!(!compiled, "the compile must fail: {said}");
    assert!(artifact.is_none(), "and write no artifact");
    for needle in [
        "Order.lines",
        "OrderLine",
        "tenant_id",
        "scoped_like_parent",
    ] {
        assert!(said.contains(needle), "names `{needle}`: {said}");
    }
}

#[test]
fn the_declaration_reaches_the_compiled_field() {
    let (compiled, said, artifact) =
        compile(&document(&json!({ "scoped_like_parent": true }), &tenant()));

    assert!(compiled, "a declared embedding compiles: {said}");
    let artifact = artifact.unwrap();
    let order = artifact.find_type("Order").unwrap();
    let declared = |name: &str| {
        order
            .fields
            .iter()
            .find(|f| f.name.as_str() == name)
            .unwrap()
            .scoped_like_parent
    };
    assert!(declared("lines"), "the declaration is carried into the compiled schema");
    assert!(!declared("id"), "and only onto the field that made it");
}

#[test]
fn a_declaration_the_document_does_not_bear_out_does_not_compile() {
    let (compiled, said, _) =
        compile(&document(&json!({ "scoped_like_parent": true }), &json!({})));

    assert!(!compiled, "the compile must fail: {said}");
    assert!(said.contains("Order.lines") && said.contains("`orders`"), "{said}");
}
