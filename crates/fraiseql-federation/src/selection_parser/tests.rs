#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_parse_simple_field_selection() {
    let query = r"
            query {
                _entities(representations: [...]) {
                    __typename
                    id
                    name
                }
            }
        ";

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("__typename"));
    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
}

#[test]
fn test_parse_inline_fragment_selection() {
    // `_entities` selections use `... on TypeName { fields }`. The spread (`...`), the
    // `on` keyword, and the type condition must not be treated as selectable fields —
    // otherwise they leak into the generated SQL SELECT list and break the query.
    let query = r"
        query($representations: [_Any!]!) {
            _entities(representations: $representations) {
                ... on User { id name }
            }
        }
    ";

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("id"), "id should be selected: {selection:?}");
    assert!(selection.contains("name"), "name should be selected: {selection:?}");
    assert!(!selection.contains("on"), "the `on` keyword is not a field: {selection:?}");
    assert!(!selection.contains("User"), "the type condition is not a field: {selection:?}");
    assert!(!selection.contains("..."), "the spread is not a field: {selection:?}");
}

#[test]
fn test_field_selection_without_whitespace() {
    let query = "{ _entities(representations: [...]) { id name email } }";

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    assert!(selection.contains("email"));
}

#[test]
fn test_field_selection_contains() {
    let mut selection = FieldSelection::new(vec!["id".to_string(), "name".to_string()]);
    assert!(selection.contains("id"));
    assert!(!selection.contains("email"));

    selection.add_field("email".to_string());
    assert!(selection.contains("email"));
}

#[test]
fn test_field_selection_excludes_invalid_patterns() {
    let query = r#"
            query {
                _entities(representations: [...]) {
                    id
                    user(id: "123") @include(if: true)
                    name
                }
            }
        "#;

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    // Should not include the function call or directive
}

/// Regression for #512: a minifying federation gateway (Hive/Apollo Router)
/// sends the `_entities` query with no spaces around the type condition or the
/// selection braces (`...on User{name email}`). The char scanner used to fuse
/// the type condition `User` onto the first field `name`, projecting a
/// non-existent column and returning the first field of every entity as null.
#[test]
fn test_minified_inline_fragment_selection() {
    let query = r"query($representations:[_Any!]!){_entities(representations:$representations){...on User{name email}}}";

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("name"), "name should be selected: {selection:?}");
    assert!(selection.contains("email"), "email should be selected: {selection:?}");
    // The fused token `Username` must never appear — that is the bug.
    assert!(
        !selection.contains("Username"),
        "type condition fused onto field: {selection:?}"
    );
    // The inline-fragment scaffolding must not leak into the SQL projection.
    assert!(!selection.contains("User"), "type condition is not a field: {selection:?}");
    assert!(!selection.contains("on"), "`on` keyword is not a field: {selection:?}");
}

/// The defect is positional, not field-name-specific: the first field is lost
/// regardless of which field it is. Cover the three shapes from the issue.
#[test]
fn test_minified_first_field_not_dropped_for_any_name() {
    let cases = [
        ("{_entities(representations:$r){...on User{name}}}", "name"),
        ("{_entities(representations:$r){...on User{email}}}", "email"),
        ("{_entities(representations:$r){...on User{id}}}", "id"),
    ];
    for (query, first_field) in cases {
        let selection = parse_field_selection(query).unwrap();
        assert!(
            selection.contains(first_field),
            "{first_field:?} should be selected for {query:?}: {selection:?}"
        );
        assert!(
            !selection.contains(&format!("User{first_field}")),
            "type condition fused onto {first_field:?} for {query:?}: {selection:?}"
        );
    }
}

/// A space-separated spread with no space before the body brace
/// (`... on User{name}`) is another shape minifiers emit — the type condition
/// must still be flushed before the body and skipped.
#[test]
fn test_spread_spaced_but_brace_minified() {
    let query = "{_entities(representations:$r){... on User{name email}}}";

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("name"), "name should be selected: {selection:?}");
    assert!(selection.contains("email"), "email should be selected: {selection:?}");
    assert!(
        !selection.contains("Username"),
        "type condition fused onto field: {selection:?}"
    );
    assert!(!selection.contains("User"), "type condition is not a field: {selection:?}");
}

/// Heterogeneous `_entities` with multiple back-to-back minified inline
/// fragments must keep the first field of *each* fragment.
#[test]
fn test_minified_heterogeneous_inline_fragments() {
    let query = "{_entities(representations:$r){...on User{name email}...on Product{title}}}";

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("name"), "User.name should be selected: {selection:?}");
    assert!(selection.contains("email"), "User.email should be selected: {selection:?}");
    assert!(selection.contains("title"), "Product.title should be selected: {selection:?}");
    assert!(!selection.contains("Username"), "User fused onto name: {selection:?}");
    assert!(!selection.contains("Producttitle"), "Product fused onto title: {selection:?}");
    assert!(!selection.contains("User"), "User type condition is not a field: {selection:?}");
    assert!(
        !selection.contains("Product"),
        "Product type condition is not a field: {selection:?}"
    );
}
