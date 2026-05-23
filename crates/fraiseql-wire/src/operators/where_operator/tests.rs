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
