//! The published `SortDirection`: the two directions, and each with the placement of its
//! NULLs. No `orderBy` item takes a separate `nulls`.

use super::{derived, field, input, schema};
use crate::schema::{EnumDefinition, EnumValueDefinition};

#[test]
fn sort_direction_publishes_the_two_directions_and_the_four_placements() {
    let d = derived(&schema());
    let values: Vec<&str> = d
        .enums
        .iter()
        .find(|e| e.name == "SortDirection")
        .expect("the direction enum is part of the sort surface")
        .values
        .iter()
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(
        values,
        [
            "ASC",
            "DESC",
            "ASC_NULLS_FIRST",
            "ASC_NULLS_LAST",
            "DESC_NULLS_FIRST",
            "DESC_NULLS_LAST"
        ]
    );
}

#[test]
fn an_order_by_item_is_field_and_direction_only() {
    let d = derived(&schema());
    let item = input(&d, "OrderOrderByInput");
    let names: Vec<&str> = item.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["field", "direction"]);
    assert_eq!(field(item, "direction").field_type, "SortDirection");
    assert!(d.enums.iter().all(|e| e.name != "NullsOrder"));
}

#[test]
fn a_schema_that_declares_sort_direction_keeps_its_own() {
    let mut declared = schema();
    declared.enums.push(
        EnumDefinition::new("SortDirection")
            .with_value(EnumValueDefinition::new("ASC"))
            .with_value(EnumValueDefinition::new("DESC")),
    );
    assert!(derived(&declared).enums.iter().all(|e| e.name != "SortDirection"));
}
