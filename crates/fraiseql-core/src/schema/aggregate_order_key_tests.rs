//! `<Name>AggregateOrderByField` offers the keys the engine sorts by: the group keys and the
//! scalar aggregates, not `json_agg`, `jsonb_agg` or `array_agg`.

use super::*;

#[test]
fn the_order_by_field_enum_lists_scalar_aggregates_and_omits_collections() {
    let schema = load(&fixture());
    let keys: Vec<String> = introspected(&schema, "SalesAggregateOrderByField")
        .enum_values
        .unwrap()
        .into_iter()
        .map(|v| v.name)
        .collect();
    for key in [
        "category",
        "count",
        "revenue_sum",
        "region_string_agg",
        "active_bool_and",
    ] {
        assert!(keys.iter().any(|k| k == key), "`{key}` is a key: {keys:?}");
    }
    for key in [
        "revenue_json_agg",
        "revenue_jsonb_agg",
        "region_array_agg",
        "region_json_agg",
    ] {
        assert!(!keys.iter().any(|k| k == key), "`{key}` is not a key: {keys:?}");
    }
    let row = introspected(&schema, "SalesAggregateRow");
    for field in ["revenue_json_agg", "region_array_agg"] {
        assert!(field_type(&row, field).is_some(), "`{field}` is still computed");
    }
}
