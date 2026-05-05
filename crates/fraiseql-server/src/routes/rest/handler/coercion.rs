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
