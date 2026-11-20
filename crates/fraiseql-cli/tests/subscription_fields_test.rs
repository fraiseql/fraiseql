//! A subscription's compile-time `fields` are checked against its return type (#2123).
//!
//! A subscription served without a plan delivers each listed field from the event's image,
//! and one served through a plan accepts only a selection the list holds. A name no field of
//! the type has is in no event, so the compile refuses it. A field the type embeds is in a
//! mutation's event and not in a captured row, so on a type with a table captured as its
//! row the compile warns.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::{Value, json};

/// `Order { id, reviewStatus, customer: Customer }` and `orderChanged` delivering
/// `fields`; `row_captured` captures `Order` from `tb_order` as its row.
fn schema(fields: &[&str], row_captured: bool) -> Value {
    let mut schema = json!({
        "types": [
            {"name": "Customer", "sql_source": "v_customer", "fields": [
                {"name": "id", "type": "ID", "nullable": false}
            ]},
            {"name": "Order", "sql_source": "v_order", "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "reviewStatus", "type": "String", "nullable": true},
                {"name": "customer", "type": "Customer", "nullable": true}
            ]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "sql_source": "v_order"}
        ],
        "subscriptions": [
            {"name": "orderChanged", "return_type": "Order", "fields": fields}
        ]
    });
    if row_captured {
        schema["types"][1]["subscribable_tables"] = json!(["tb_order"]);
    }
    schema
}

/// What `fraiseql compile` printed for `schema`, and whether it compiled.
fn compile_output(schema: &Value) -> (bool, String) {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    std::fs::write(dir.path().join("fraiseql.toml"), "[project]\nname = \"app\"\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["compile", "schema.json", "--output", "schema.compiled.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), printed)
}

/// Every warning names the subscription's field the same way, so a test can say none was.
const DELIVERS: &str = "delivers `";

#[test]
fn a_field_the_type_does_not_declare_is_refused() {
    let (compiled, printed) = compile_output(&schema(&["id", "totl"], false));
    assert!(!compiled, "{printed}");
    for part in ["orderChanged", "`totl`", "`Order`"] {
        assert!(printed.contains(part), "names {part}: {printed}");
    }
}

/// A subscription lists the names it publishes; the stored key is how the image holds it.
#[test]
fn a_stored_key_is_refused_naming_the_published_name() {
    let (compiled, printed) = compile_output(&schema(&["id", "review_status"], false));
    assert!(!compiled, "{printed}");
    assert!(printed.contains("`reviewStatus`"), "{printed}");
}

#[test]
fn an_embedded_field_of_a_row_captured_type_compiles_with_a_warning() {
    let (compiled, printed) = compile_output(&schema(&["id", "customer"], true));
    assert!(compiled, "{printed}");
    for part in [
        "orderChanged",
        "delivers `customer`",
        "`Order`",
        "`tb_order`",
    ] {
        assert!(printed.contains(part), "names {part}: {printed}");
    }
}

/// No `fields` delivers every field the type declares, the embedded one included.
#[test]
fn a_subscription_listing_no_fields_is_warned_of_for_each_embedded_field() {
    let (compiled, printed) = compile_output(&schema(&[], true));
    assert!(compiled, "{printed}");
    assert!(printed.contains("delivers `customer`"), "{printed}");
    assert!(
        !printed.contains("delivers `reviewStatus`"),
        "a column carries a scalar: {printed}"
    );
}

#[test]
fn the_same_fields_compile_without_a_word_where_only_the_document_is_captured() {
    for fields in [&["id", "reviewStatus", "customer"][..], &[][..]] {
        let (compiled, printed) = compile_output(&schema(fields, false));
        assert!(compiled, "{printed}");
        assert!(!printed.contains(DELIVERS), "{printed}");
    }
}
