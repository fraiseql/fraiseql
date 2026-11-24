//! `?sort=`'s NULL placement: `.nullsfirst` and `.nullslast` after a sort field.

use super::{extractor_list, list_query_def, test_config, user_type_def};

fn order_by(sort: &str) -> Result<Option<serde_json::Value>, String> {
    let config = test_config();
    let (qd, td) = (list_query_def(), user_type_def());
    extractor_list(&config, &qd, &td)
        .extract(&[], &[("sort", sort)])
        .map(|params| params.order_by)
        .map_err(|e| e.to_string())
}

#[test]
fn a_placement_suffix_becomes_the_direction_that_names_it() {
    assert_eq!(
        order_by("-age.nullslast,name.nullsfirst,email").unwrap(),
        Some(serde_json::json!([
            { "field": "age", "direction": "DESC_NULLS_LAST" },
            { "field": "name", "direction": "ASC_NULLS_FIRST" },
            { "field": "email", "direction": "ASC" },
        ]))
    );
}

#[test]
fn any_other_suffix_is_refused_naming_the_two() {
    let message = order_by("-age.nullsmiddle").unwrap_err();
    assert!(message.contains("`.nullsmiddle` is not a NULL placement"), "{message}");
    assert!(message.contains("`.nullsfirst` or `.nullslast`"), "{message}");
}

/// The field is still checked: a placement does not let an unknown field through.
#[test]
fn a_placement_on_an_unknown_field_is_refused_for_the_field() {
    let message = order_by("bogus.nullslast").unwrap_err();
    assert!(message.contains("Unknown field 'bogus'"), "{message}");
}
