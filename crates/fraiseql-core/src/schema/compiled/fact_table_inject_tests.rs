//! Load-time refusals of a fact table's declared `inject_params` (#1800).
//!
//! A declaration the runtime cannot honour as written never loads: a filter on a column the
//! table does not have, an optional claim, a relation declared to scope itself, and a source
//! contradicting the linked type's. An inheritance the table cannot take is not refused here:
//! the schemas that load today load as before.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::{Value, json};

use super::CompiledSchema;

/// `tf_sales` with the filter columns `tenant_id` and `author_id`, read as type `Sale`.
fn fixture() -> Value {
    json!({
        "types": [
            {"name": "Sale", "sql_source": "v_sale",
             "fields": [{"name": "revenue", "field_type": "Float", "nullable": false},
                        {"name": "tenant_id", "field_type": "String", "nullable": false},
                        {"name": "author_id", "field_type": "String", "nullable": false}]}
        ],
        "queries": [
            {"name": "sales", "return_type": "Sale", "returns_list": true, "sql_source": "v_sale"}
        ],
        "mutations": [],
        "fact_tables": {
            "tf_sales": {
                "table_name": "tf_sales",
                "type_name": "Sale",
                "measures": [{"name": "revenue", "sql_type": "Decimal", "nullable": false}],
                "dimensions": {"name": "data", "paths": []},
                "denormalized_filters": [
                    {"name": "tenant_id", "sql_type": "Text", "indexed": true},
                    {"name": "author_id", "sql_type": "Text", "indexed": true}
                ]
            }
        }
    })
}

fn jwt(claim: &str) -> Value {
    json!({"source": "jwt", "claim": claim})
}

fn load(doc: &Value) -> crate::error::Result<CompiledSchema> {
    CompiledSchema::from_json(&doc.to_string(), false)
}

fn refusal(mutate: impl FnOnce(&mut Value)) -> String {
    let mut doc = fixture();
    mutate(&mut doc);
    load(&doc)
        .expect_err("a declaration the runtime cannot honour must not load")
        .to_string()
}

fn declare(doc: &mut Value, column: &str, source: Value) {
    doc["fact_tables"]["tf_sales"]["inject_params"][column] = source;
}

/// The positive control: without it every refusal below would pass against a check that
/// refused every declaration.
#[test]
fn a_declaration_on_a_filter_column_loads_and_survives() {
    let mut doc = fixture();
    declare(&mut doc, "tenant_id", jwt("org"));
    let schema = load(&doc).expect("a filter on a filter column loads");
    let table = schema.get_fact_table("tf_sales").unwrap();
    assert_eq!(
        table.inject_params.get("tenant_id"),
        Some(&crate::schema::InjectedParamSource::Jwt("org".to_string()))
    );
    let round_trip = serde_json::to_value(&schema).unwrap();
    assert_eq!(round_trip["fact_tables"]["tf_sales"]["inject_params"]["tenant_id"], jwt("org"));
}

#[test]
fn a_declaration_on_a_column_the_table_lacks_is_refused() {
    let message = refusal(|d| declare(d, "owner_id", jwt("org")));
    for part in [
        "fact table 'tf_sales'",
        "inject param 'owner_id'",
        "not one of its denormalized filter columns",
        "'author_id', 'tenant_id'",
    ] {
        assert!(message.contains(part), "{part:?}: {message}");
    }
}

#[test]
fn an_optional_claim_is_refused() {
    let message =
        refusal(|d| declare(d, "tenant_id", json!({"source": "jwt_optional", "claim": "org"})));
    for part in [
        "fact table 'tf_sales'",
        "inject param 'tenant_id'",
        "jwt_optional:org",
        "jwt:org",
    ] {
        assert!(message.contains(part), "{part:?}: {message}");
    }
}

#[test]
fn a_declaration_on_a_relation_scoped_table_is_refused() {
    let message = refusal(|d| {
        declare(d, "tenant_id", jwt("org"));
        d["fact_tables"]["tf_sales"]["row_scope"] = "relation".into();
    });
    for part in [
        "fact table 'tf_sales'",
        "row_scope = \"relation\"",
        "inject_params",
    ] {
        assert!(message.contains(part), "{part:?}: {message}");
    }
}

#[test]
fn a_declaration_contradicting_the_linked_type_is_refused() {
    let message = refusal(|d| {
        declare(d, "tenant_id", jwt("org"));
        d["types"][0]["inject_params"] = json!({"tenant_id": jwt("tid")});
    });
    for part in [
        "fact table 'tf_sales'",
        "type 'Sale'",
        "'tenant_id'",
        "jwt claim 'org'",
        "jwt claim 'tid'",
    ] {
        assert!(message.contains(part), "{part:?}: {message}");
    }
}

/// Saying the same thing twice is redundant, not contradictory.
#[test]
fn a_declaration_agreeing_with_the_linked_type_loads() {
    let mut doc = fixture();
    declare(&mut doc, "tenant_id", jwt("org"));
    doc["types"][0]["inject_params"] = json!({"tenant_id": jwt("org")});
    load(&doc).expect("the same source in both places loads");
}

/// A type injecting a column the fact table lacks loads, as it does today: the inheritance
/// does not apply, and the runtime refuses the roots as #1799 does, saying why.
#[test]
fn an_inheritance_the_table_cannot_take_still_loads() {
    let mut doc = fixture();
    doc["types"][0]["inject_params"] = json!({"owner_id": jwt("org")});
    doc["types"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "owner_id", "field_type": "String", "nullable": false}));
    let schema = load(&doc).expect("no new requirement on a schema that loads today");
    let reason = schema
        .unenforced_tenant_scope(schema.get_fact_table("tf_sales").unwrap())
        .expect("still tenant-scoped, and nothing scopes it");
    for part in [
        "inherited from type 'Sale'",
        "'owner_id' is not one of its denormalized filter columns",
    ] {
        assert!(reason.contains(part), "{part:?}: {reason}");
    }
}

/// #1799's refusal narrows: a fact table its own or its type's `inject_params` scope is no
/// longer unscoped.
#[test]
fn a_scoped_fact_table_is_not_unenforced() {
    let mut declared = fixture();
    declare(&mut declared, "tenant_id", jwt("org"));
    let mut inherited = fixture();
    inherited["types"][0]["inject_params"] = json!({"tenant_id": jwt("org")});
    for (what, doc) in [("declared", declared), ("inherited", inherited)] {
        let schema = load(&doc).unwrap();
        assert_eq!(
            schema.unenforced_tenant_scope(schema.get_fact_table("tf_sales").unwrap()),
            None,
            "{what}"
        );
        assert!(schema.fact_table_scope_violations().is_empty(), "{what}");
    }
}

/// The control for the narrowing: a type injecting a filter column through a query, with no
/// declaration anywhere on the fact table or its type, is still refused (never inferred).
#[test]
fn a_column_injected_only_by_a_query_is_still_unenforced() {
    let mut doc = fixture();
    doc["queries"][0]["inject_params"] = json!({"tenant_id": jwt("org")});
    let schema = load(&doc).unwrap();
    assert!(
        schema
            .unenforced_tenant_scope(schema.get_fact_table("tf_sales").unwrap())
            .is_some(),
        "scoping is read off a declaration, never off another operation's"
    );
}
