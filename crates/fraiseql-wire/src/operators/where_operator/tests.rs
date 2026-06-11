#![allow(clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_operator_names() {
    let op = WhereOperator::Eq(Field::JsonbField("id".to_string()), Value::Number(1.0));
    assert_eq!(op.name(), "Eq");

    let op = WhereOperator::LenGt(Field::JsonbField("tags".to_string()), 5);
    assert_eq!(op.name(), "LenGt");
}

#[test]
fn test_operator_validation() {
    let op = WhereOperator::Eq(
        Field::JsonbField("name".to_string()),
        Value::String("John".to_string()),
    );
    op.validate()
        .unwrap_or_else(|e| panic!("expected Ok for valid field 'name': {e}"));

    let op = WhereOperator::Eq(
        Field::JsonbField("bad-name".to_string()),
        Value::String("John".to_string()),
    );
    let result = op.validate();
    assert!(
        result.is_err(),
        "expected Err for invalid field 'bad-name', got: {result:?}"
    );
}

#[test]
fn test_vector_operator_creation() {
    let op = WhereOperator::L2Distance {
        field: Field::JsonbField("embedding".to_string()),
        vector: vec![0.1, 0.2, 0.3],
        threshold: 0.5,
    };
    assert_eq!(op.name(), "L2Distance");
}

#[test]
fn test_network_operator_creation() {
    let op = WhereOperator::InSubnet {
        field: Field::JsonbField("ip".to_string()),
        subnet: "192.168.0.0/24".to_string(),
    };
    assert_eq!(op.name(), "InSubnet");
}

#[test]
fn test_full_text_search_language_rejected_by_validate() {
    // H41: the regconfig is spliced into plainto_tsquery('{lang}', $n), so a
    // hostile language must be rejected by validate() before SQL generation.
    let hostile = WhereOperator::Matches {
        field: Field::JsonbField("body".to_string()),
        query: "hello".to_string(),
        language: Some("english', $1) OR 1=1 --".to_string()),
    };
    assert!(
        hostile.validate().is_err(),
        "hostile full-text language must be rejected"
    );

    // Legitimate regconfigs and the default (None) are accepted.
    let ok = WhereOperator::WebsearchQuery {
        field: Field::JsonbField("body".to_string()),
        query: "hello".to_string(),
        language: Some("english".to_string()),
    };
    ok.validate()
        .unwrap_or_else(|e| panic!("expected Ok for 'english': {e}"));

    let default = WhereOperator::PhraseQuery {
        field: Field::JsonbField("body".to_string()),
        query: "hello world".to_string(),
        language: None,
    };
    default
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for None language: {e}"));
}

#[test]
fn test_validate_text_search_language_helper() {
    assert!(WhereOperator::validate_text_search_language(None).is_ok());
    assert!(WhereOperator::validate_text_search_language(Some("english")).is_ok());
    assert!(WhereOperator::validate_text_search_language(Some("simple")).is_ok());
    assert!(WhereOperator::validate_text_search_language(Some("german_de")).is_ok());

    for bad in [
        "english', $1) OR 1=1 --",
        "",
        "English",
        "en glish",
        "a'b",
        "x;y",
    ] {
        assert!(
            WhereOperator::validate_text_search_language(Some(bad)).is_err(),
            "language {bad:?} must be rejected"
        );
    }
}
