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
