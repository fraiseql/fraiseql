//! The gates and calendar a linked grain's two tables must share, which `schema.json`
//! cannot all express: checked on the metadata directly.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use serde_json::{Value, json};

use super::linked_grain_violations;
use crate::compiler::fact_table::FactTableMetadata;

fn table(name: &str, grain: &Value) -> Value {
    json!({
        "table_name": name,
        "measures": [{"name": "revenue", "sql_type": "BigInt", "nullable": false}],
        "dimensions": {"name": "data", "paths": []},
        "denormalized_filters": [
            {"name": "day", "sql_type": "Date", "indexed": true},
            {"name": "org", "sql_type": "Text", "indexed": true}
        ],
        "grain": grain
    })
}

/// The violations of `tf_m` linked to `tf_d`, each changed by `edit` first.
fn violations(edit: impl FnOnce(&mut Value, &mut Value)) -> Vec<String> {
    let mut month =
        table("tf_m", &json!({"column": "day", "bucket": "month", "edges_from": "tf_d"}));
    let mut day = table("tf_d", &json!({"column": "day", "bucket": "day"}));
    edit(&mut month, &mut day);
    let tables: HashMap<String, FactTableMetadata> = [month, day]
        .into_iter()
        .map(|t| {
            let t: FactTableMetadata = serde_json::from_value(t).unwrap();
            (t.table_name.clone(), t)
        })
        .collect();
    linked_grain_violations(&tables)
}

#[test]
fn two_tables_alike_at_nesting_grains_link() {
    assert_eq!(violations(|_, _| {}), Vec::<String>::new());
}

#[test]
fn the_finer_table_is_read_under_the_coarse_tables_gates_and_calendar() {
    let gates = "fact table `tf_m`: `grain.edges_from` `tf_d` is read under other gates than \
                 `tf_m` (`type_name`, `row_scope` and `inject_params` must be equal): its rows \
                 would answer callers under rules they are not held to";
    let calendar = "fact table `tf_m`: `grain.edges_from` `tf_d` declares other \
                    `calendar_dimensions`";
    for (edit, expected) in [
        (
            Box::new(|_: &mut Value, day: &mut Value| day["type_name"] = json!("Sale"))
                as Box<dyn FnOnce(&mut Value, &mut Value)>,
            gates,
        ),
        (
            Box::new(|month: &mut Value, _: &mut Value| month["type_name"] = json!("Sale")),
            gates,
        ),
        (
            Box::new(|_: &mut Value, day: &mut Value| {
                day["inject_params"] = json!({"org": {"source": "jwt", "claim": "org_id"}});
            }),
            gates,
        ),
        (
            Box::new(|_: &mut Value, day: &mut Value| day["row_scope"] = json!("relation")),
            gates,
        ),
        (
            Box::new(|_: &mut Value, day: &mut Value| {
                day["calendar_dimensions"] = json!([{
                    "source_column": "org",
                    "granularities": [{"column_name": "org_info", "buckets": []}]
                }]);
            }),
            calendar,
        ),
    ] {
        assert_eq!(violations(edit), vec![expected.to_string()]);
    }
}
