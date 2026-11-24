//! A list query's `orderBy` publishes `SortDirection` with the six values the engine sorts by.
//! A declared `SortDirection` takes the derived one's place, so it serves only when it holds
//! those six: any other declaration is refused when the schema loads.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use super::CompiledSchema;

/// `users`, sortable, and a declared `SortDirection` of `values` when given.
fn schema_json(values: Option<&[&str]>) -> String {
    let enums = values.map_or_else(String::new, |values| {
        let members: Vec<String> = values.iter().map(|v| format!(r#"{{"name":"{v}"}}"#)).collect();
        format!(r#""enums": [{{"name":"SortDirection","values":[{}]}}],"#, members.join(","))
    });
    format!(
        r#"{{
        "types": [{{"name":"User","sql_source":"v_user","fields":[
            {{"name":"id","field_type":"ID"}},{{"name":"name","field_type":"String"}}]}}],
        {enums}
        "queries": [{{"name":"users","return_type":"User","returns_list":true,
            "sql_source":"v_user","auto_params":{{"has_order_by":true}}}}]
    }}"#
    )
}

fn published(json: &str) -> Vec<String> {
    let schema = CompiledSchema::from_json(json, false).unwrap();
    schema
        .find_enum("SortDirection")
        .unwrap()
        .values
        .iter()
        .map(|v| v.name.clone())
        .collect()
}

#[test]
fn the_derived_sort_direction_holds_the_six() {
    assert_eq!(published(&schema_json(None)), crate::types::SORT_DIRECTIONS);
}

#[test]
fn a_declared_sort_direction_of_the_six_serves() {
    assert_eq!(
        published(&schema_json(Some(&crate::types::SORT_DIRECTIONS))),
        crate::types::SORT_DIRECTIONS
    );
}

#[test]
fn a_declared_sort_direction_of_other_values_is_refused() {
    for values in [&["ASC", "DESC"][..], &["DESC", "ASC"], &["UP", "DOWN"]] {
        let message = CompiledSchema::from_json(&schema_json(Some(values)), false)
            .map(|_| panic!("{values:?} loaded"))
            .unwrap_err()
            .to_string();
        assert!(
            message.contains(
                "`SortDirection` is declared, and the `orderBy` of query `users` publishes the \
                 enum of that name with the values `ASC`, `DESC`, `ASC_NULLS_FIRST`, \
                 `ASC_NULLS_LAST`, `DESC_NULLS_FIRST`, `DESC_NULLS_LAST`: declare those, or no \
                 `SortDirection`, and recompile"
            ),
            "{values:?}: {message}"
        );
    }
}
