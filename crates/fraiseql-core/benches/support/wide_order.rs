//! An `Order` row of a chosen width, its schema and a selection of every field, for the
//! measures of the selection-driven projector: the allocation count in `memory_profile` and
//! the time in `project_entity`.

use fraiseql_core::{
    graphql::FieldSelection, runtime::stored_key_candidates, schema::CompiledSchema,
};
use serde_json::{Map, Value, json};

/// A field name of more than one word, so its `snake_case` and `camelCase` spellings differ.
/// Every name has the same length, so each costs the same to resolve.
pub fn declared(i: usize) -> String {
    format!("lineTotal{i:02}")
}

/// The key the field `declared(i)` is stored under, by the rule the projectors read with.
pub fn stored(i: usize) -> String {
    stored_key_candidates(&declared(i)).0
}

/// An `Order` with `width` integer fields and a `shippingAddress` holding `width` more.
pub fn schema(width: usize) -> CompiledSchema {
    let ints: Vec<Value> =
        (0..width).map(|i| json!({"name": declared(i), "field_type": "Int"})).collect();
    let mut order = ints.clone();
    order.push(json!({"name": "shippingAddress", "field_type": {"Object": "Address"}}));
    let text = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order", "jsonb_column": "data", "fields": order},
            {"name": "Address", "sql_source": "v_address", "jsonb_column": "data", "fields": ints},
        ],
        "queries": [],
    })
    .to_string();
    CompiledSchema::from_json(&text, false).expect("the measurement schema loads")
}

pub const fn selection(name: String, nested_fields: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name,
        alias: None,
        arguments: Vec::new(),
        nested_fields,
        directives: Vec::new(),
    }
}

pub fn selections(width: usize) -> Vec<FieldSelection> {
    let leaves = || (0..width).map(|i| selection(declared(i), Vec::new()));
    leaves()
        .chain([selection("shippingAddress".into(), leaves().collect())])
        .collect()
}

/// A stored row, keyed as a view stores it: `snake_case`.
pub fn row(width: usize) -> Value {
    let ints = || (0..width).map(|i| (stored(i), json!(i))).collect::<Map<String, Value>>();
    let mut order = ints();
    order.insert("shipping_address".into(), Value::Object(ints()));
    Value::Object(order)
}
