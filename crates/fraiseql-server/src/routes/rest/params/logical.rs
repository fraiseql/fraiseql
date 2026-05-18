//! Logical operator parsing for REST queries.
//!
//! Supports `?or=()`, `?and=()`, `?not=()` syntax with nested operators.

use fraiseql_error::FraiseQLError;

/// Maximum nesting depth for logical operator groups (`or=()`, `and=()`, `not=()`).
const MAX_LOGICAL_DEPTH: usize = 64;

/// Parse a logical operator group value like `(name[eq]=Alice,name[eq]=Bob)`.
///
/// Returns a JSON value like `{"_or": [{"name": {"eq": "Alice"}}, {"name": {"eq": "Bob"}}]}`.
///
/// Supports nesting: `(and=(age[gte]=18,active[eq]=true),name[eq]=admin)` produces
/// `{"_or": [{"_and": [...]}, {"name": {"eq": "admin"}}]}`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if:
/// - Input is not enclosed in parentheses
/// - Nesting depth exceeds `MAX_LOGICAL_DEPTH`
pub fn parse_logical_group(
    input: &str,
    dsl_key: &str,
    depth: usize,
) -> Result<serde_json::Value, FraiseQLError> {
    use crate::routes::rest::params::{bracket::parse_bracket_key, helpers::validation_error};

    if depth > MAX_LOGICAL_DEPTH {
        return Err(validation_error(format!(
            "Logical operator nesting depth exceeds maximum ({MAX_LOGICAL_DEPTH})."
        )));
    }

    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return Err(validation_error(format!(
            "Logical operator value must be enclosed in parentheses: `{dsl_key}=(...)`. \
             Got: `{trimmed}`"
        )));
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let parts = split_logical_parts(inner);

    let mut conditions = Vec::with_capacity(parts.len());
    for part in &parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Check for nested logical operator: `and=(...)` or `or=(...)` or `not=(...)`
        if let Some((nested_op, nested_val)) = parse_nested_logical(part) {
            let nested_key = format!("_{nested_op}");
            let nested = parse_logical_group(nested_val, &nested_key, depth + 1)?;
            conditions.push(nested);
        } else if let Some((field_op, value)) = part.split_once('=') {
            let json_val = parse_logical_value(value);
            if let Some((field, op)) = parse_bracket_key(field_op) {
                // Bracket condition: `field[op]=value`
                conditions.push(serde_json::json!({ field: { op: json_val } }));
            } else {
                // Simple equality: `field=value`
                conditions.push(serde_json::json!({ field_op: { "eq": json_val } }));
            }
        }
    }

    Ok(serde_json::json!({ dsl_key: conditions }))
}

/// Split logical group contents by commas, respecting nested parentheses.
pub fn split_logical_parts(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            },
            ')' => {
                depth -= 1;
                current.push(ch);
            },
            ',' if depth == 0 => {
                parts.push(current.clone());
                current.clear();
            },
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Check if a part is a nested logical operator: `and=(...)`, `or=(...)`, `not=(...)`.
pub fn parse_nested_logical(part: &str) -> Option<(&str, &str)> {
    for op in &["and", "or", "not"] {
        let prefix = format!("{op}=");
        if let Some(rest) = part.strip_prefix(&prefix) {
            if rest.starts_with('(') && rest.ends_with(')') {
                return Some((op, rest));
            }
        }
    }
    None
}

/// Parse a value from a logical group, attempting numeric and boolean coercion.
pub fn parse_logical_value(raw: &str) -> serde_json::Value {
    // Try integer.
    if let Ok(v) = raw.parse::<i64>() {
        return serde_json::Value::Number(v.into());
    }
    // Try float.
    if let Ok(v) = raw.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return serde_json::Value::Number(n);
        }
    }
    // Try boolean.
    match raw {
        "true" => return serde_json::Value::Bool(true),
        "false" => return serde_json::Value::Bool(false),
        _ => {},
    }
    // Default to string.
    serde_json::Value::String(raw.to_string())
}
