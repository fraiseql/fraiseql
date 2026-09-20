#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_value_to_string() {
    assert_eq!(value_to_string(&Value::String("test".to_string())).unwrap(), "test");
    assert_eq!(value_to_string(&Value::Number(789.into())).unwrap(), "789");
    assert_eq!(value_to_string(&Value::Bool(true)).unwrap(), "true");
    assert_eq!(value_to_string(&Value::Null).unwrap(), "null");
}
