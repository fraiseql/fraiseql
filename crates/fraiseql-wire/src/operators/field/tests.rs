use super::*;

#[test]
fn test_valid_field_names() {
    assert!(is_valid_field_name("name"));
    assert!(is_valid_field_name("_private"));
    assert!(is_valid_field_name("field_123"));
    assert!(is_valid_field_name("a"));
}

#[test]
fn test_invalid_field_names() {
    assert!(!is_valid_field_name(""));
    assert!(!is_valid_field_name("123field")); // starts with digit
    assert!(!is_valid_field_name("field-name")); // contains dash
    assert!(!is_valid_field_name("field.name")); // contains dot
    assert!(!is_valid_field_name("field'name")); // contains quote
}

#[test]
fn test_field_validation() {
    Field::JsonbField("name".to_string())
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for valid field 'name': {e}"));

    let result = Field::JsonbField("name-invalid".to_string()).validate();
    assert!(
        result.is_err(),
        "expected Err for field 'name-invalid', got: {result:?}"
    );

    Field::JsonbPath(vec!["user".to_string(), "name".to_string()])
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for valid JsonbPath [user, name]: {e}"));
}

#[test]
fn test_field_to_sql_jsonb() {
    let field = Field::JsonbField("name".to_string());
    assert_eq!(field.to_sql(), "(data->'name')");
}

#[test]
fn test_field_to_sql_direct() {
    let field = Field::DirectColumn("created_at".to_string());
    assert_eq!(field.to_sql(), "created_at");
}

#[test]
fn test_field_to_sql_path() {
    let field = Field::JsonbPath(vec!["user".to_string(), "name".to_string()]);
    assert_eq!(field.to_sql(), "(data->'user'->>'name')");
}

#[test]
fn test_value_to_sql_literal() {
    assert_eq!(Value::String("test".to_string()).to_sql_literal(), "'test'");
    assert_eq!(Value::Number(42.0).to_sql_literal(), "42");
    assert_eq!(Value::Bool(true).to_sql_literal(), "true");
    assert_eq!(Value::Null.to_sql_literal(), "NULL");
}

#[test]
fn test_value_string_escaping() {
    let val = Value::String("O'Brien".to_string());
    assert_eq!(val.to_sql_literal(), "'O''Brien'");
}
