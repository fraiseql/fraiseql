//! Path parameter type coercion.

use serde_json::json;

/// Coerce a path parameter string to an appropriate JSON value.
///
/// Attempts integer, then boolean, then falls back to string.
pub(super) fn coerce_path_param_value(value: &str) -> serde_json::Value {
    if let Ok(n) = value.parse::<i64>() {
        return json!(n);
    }
    match value {
        "true" => return json!(true),
        "false" => return json!(false),
        _ => {},
    }
    json!(value)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
#[allow(clippy::missing_panics_doc)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn coerce_path_param_value_integer() {
        let val = coerce_path_param_value("42");
        assert_eq!(val, json!(42i64));
    }

    #[test]
    fn coerce_path_param_value_boolean_true() {
        let val = coerce_path_param_value("true");
        assert_eq!(val, json!(true));
    }

    #[test]
    fn coerce_path_param_value_boolean_false() {
        let val = coerce_path_param_value("false");
        assert_eq!(val, json!(false));
    }

    #[test]
    fn coerce_path_param_value_string() {
        let val = coerce_path_param_value("hello");
        assert_eq!(val, json!("hello"));
    }
}
