//! The fact-table roots publish the one `SortDirection`, and a declared one serves them when
//! it holds the two directions, or the two and the four placements.

use serde_json::{Value, json};

use super::{fixture, introspected, load, refusal};

fn declaring(values: &[&str]) -> Value {
    let mut doc = fixture();
    doc["enums"] = json!([{
        "name": "SortDirection",
        "values": values.iter().map(|v| json!({ "name": v })).collect::<Vec<_>>(),
    }]);
    doc
}

fn published(doc: &Value) -> Vec<String> {
    introspected(&load(doc), "SortDirection")
        .enum_values
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.name)
        .collect()
}

#[test]
fn the_roots_publish_the_six_directions() {
    assert_eq!(published(&fixture()), crate::types::SORT_DIRECTIONS);
}

#[test]
fn a_declared_sort_direction_with_the_two_or_the_six_serves() {
    assert_eq!(published(&declaring(&["ASC", "DESC"])), ["ASC", "DESC"]);
    assert_eq!(
        published(&declaring(&crate::types::SORT_DIRECTIONS)),
        crate::types::SORT_DIRECTIONS
    );
}

#[test]
fn a_declared_sort_direction_with_other_values_is_refused() {
    for values in [
        &["ASC"][..],
        &["ASC", "DESC", "ASC_NULLS_FIRST"],
        &["DESC", "ASC"],
    ] {
        let message = refusal(&declaring(values));
        assert!(message.contains("`SortDirection` is declared"), "{values:?}: {message}");
    }
}
