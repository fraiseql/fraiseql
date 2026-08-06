//! The `value_json` seam: how an inline GraphQL argument is stored and read back.
//!
//! A parsed argument is kept as a JSON string on [`GraphQLArgument::value_json`]
//! and re-read by the matcher, the directive evaluator, the query classifier and
//! the multi-root pipeline. Everything about that round trip lives here, because
//! #719 was two independent failures of it:
//!
//! * **The writer escaped only `"`.** A Windows path, a newline or a control character in a string
//!   literal produced *invalid JSON*, which the reader dropped via `.ok()?`. A dropped `where:`
//!   argument does not narrow a result set — it **widens** it.
//! * **Variables were signalled in-band as `"$name"`.** A literal `"$100"` was indistinguishable
//!   from a reference to a variable named `100`, and resolved to `null`.
//!
//! Both are fixed structurally rather than by patching the escaper: serialization
//! goes through `serde_json`, and a variable is a tagged **object**
//! `{"$var": "name"}`. GraphQL names match `[_A-Za-z][_0-9A-Za-z]*`, so no
//! client-supplied object key can ever be `$var` — the marker is unforgeable
//! rather than merely unlikely.

use std::collections::HashMap;

use fraiseql_error::{FraiseQLError, Result};
use graphql_parser::query;
use serde_json::{Map, Value};

/// Object key marking a GraphQL variable reference.
///
/// Not a valid GraphQL name, so a client cannot produce it as a literal key.
pub const VARIABLE_TAG: &str = "$var";

/// Maximum nesting depth accepted when serializing or resolving an argument.
///
/// Bounds stack use against an adversarial payload; the parser and the resolver
/// share the limit so a value that serialized cannot fail to resolve.
pub const MAX_DEPTH: usize = 64;

/// Build the JSON representation of a parsed GraphQL argument value.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the value nests deeper than
/// [`MAX_DEPTH`]. Serialization itself cannot fail: every GraphQL value has a
/// JSON counterpart.
pub fn encode(value: &query::Value<String>) -> Result<String> {
    let json = to_json(value, 0)?;
    serde_json::to_string(&json).map_err(|e| FraiseQLError::Internal {
        message: format!("failed to serialize GraphQL argument: {e}"),
        source:  None,
    })
}

fn to_json(value: &query::Value<String>, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(FraiseQLError::validation(format!(
            "GraphQL argument nests deeper than the {MAX_DEPTH}-level limit"
        )));
    }
    Ok(match value {
        query::Value::String(s) => Value::String(s.clone()),
        query::Value::Int(i) => i.as_i64().map_or(Value::Null, Value::from),
        query::Value::Float(f) => {
            serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number)
        },
        query::Value::Boolean(b) => Value::Bool(*b),
        query::Value::Null => Value::Null,
        // An enum value is a bare GraphQL name; JSON has no enum, so it travels
        // as a string and `value_type` records that it was an enum.
        query::Value::Enum(e) => Value::String(e.clone()),
        query::Value::List(items) => Value::Array(
            items.iter().map(|item| to_json(item, depth + 1)).collect::<Result<Vec<_>>>()?,
        ),
        query::Value::Object(obj) => {
            let mut map = Map::with_capacity(obj.len());
            for (k, v) in obj {
                map.insert(k.clone(), to_json(v, depth + 1)?);
            }
            Value::Object(map)
        },
        query::Value::Variable(v) => variable_ref(v),
    })
}

/// The JSON representation of a reference to variable `name`.
#[must_use]
pub fn variable_ref(name: &str) -> Value {
    let mut map = Map::with_capacity(1);
    map.insert(VARIABLE_TAG.to_string(), Value::String(name.to_string()));
    Value::Object(map)
}

/// The variable this value references, if it is a reference rather than a literal.
#[must_use]
pub fn variable_name(value: &Value) -> Option<&str> {
    let map = value.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.get(VARIABLE_TAG)?.as_str()
}

/// Read a stored `value_json` back into JSON.
///
/// # Errors
///
/// Returns `FraiseQLError::Internal` if the stored string is not valid JSON.
/// This can only happen if something wrote `value_json` without going through
/// [`encode`] — and it must be loud: the previous `.ok()?` dropped the argument,
/// and a dropped filter widens the result set instead of narrowing it.
pub fn decode(value_json: &str) -> Result<Value> {
    serde_json::from_str(value_json).map_err(|e| FraiseQLError::Internal {
        message: format!(
            "stored GraphQL argument is not valid JSON ({e}); refusing to execute the query \
             rather than dropping the argument, which would widen the result set"
        ),
        source:  None,
    })
}

/// Substitute every variable reference inside `value` with its concrete value.
///
/// An undefined variable resolves to JSON `null`, matching GraphQL's treatment
/// of an omitted nullable argument.
#[must_use]
#[allow(clippy::implicit_hasher)]
// Reason: the variables map is built by the runtime with the default hasher; a
// generic parameter here would leak into every caller for no benefit.
pub fn resolve_variables(value: Value, variables: &HashMap<String, Value>) -> Value {
    resolve_at(value, variables, 0)
}

/// Read a stored `value_json` back into JSON **with its variable references
/// substituted** — the form a consumer that wants values almost always means.
///
/// [`decode`] alone yields a value that may still contain `{"$var": "name"}`
/// markers. Every consumer that then compares, filters or authorizes on the
/// result must pair it with [`resolve_variables`]; the field authorizer did not,
/// so a policy matching on an argument compared against the marker and silently
/// took its default branch for every client that passes arguments as variables —
/// which is every generated client (#903).
///
/// Use [`decode`] directly only when the marker itself is the subject, as
/// `resolve_inline_arg` does: it distinguishes a whole-argument variable the
/// request omitted (drop the argument, fall back to the query default) from one
/// bound to null.
///
/// # Errors
///
/// Returns `FraiseQLError::Internal` if the stored string is not valid JSON.
#[allow(clippy::implicit_hasher)] // Reason: as resolve_variables — one hasher, no generic leak.
pub fn decode_resolved(value_json: &str, variables: &HashMap<String, Value>) -> Result<Value> {
    Ok(resolve_variables(decode(value_json)?, variables))
}

fn resolve_at(value: Value, variables: &HashMap<String, Value>, depth: usize) -> Value {
    if depth >= MAX_DEPTH {
        return value;
    }
    match value {
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| resolve_at(v, variables, depth + 1)).collect())
        },
        Value::Object(map) => {
            if let Some(name) = map.get(VARIABLE_TAG).and_then(Value::as_str) {
                if map.len() == 1 {
                    return variables.get(name).cloned().unwrap_or(Value::Null);
                }
            }
            Value::Object(
                map.into_iter().map(|(k, v)| (k, resolve_at(v, variables, depth + 1))).collect(),
            )
        },
        other => other,
    }
}

/// Render a stored argument value back as GraphQL source syntax.
///
/// Used by the multi-root pipeline, which re-serializes each root field into a
/// standalone query. Strings are escaped by `serde_json` — GraphQL and JSON
/// agree on string-literal syntax — and variable references become `$name`
/// rather than a quoted string.
#[must_use]
pub fn to_graphql(value: &Value) -> String {
    if let Some(name) = variable_name(value) {
        return format!("${name}");
    }
    match value {
        Value::Object(map) => {
            // GraphQL object keys are bare names, not quoted strings.
            let pairs: Vec<String> =
                map.iter().map(|(k, v)| format!("{k}: {}", to_graphql(v))).collect();
            format!("{{{}}}", pairs.join(", "))
        },
        Value::Array(items) => {
            let rendered: Vec<String> = items.iter().map(to_graphql).collect();
            format!("[{}]", rendered.join(", "))
        },
        // `serde_json` emits a correctly escaped string literal; GraphQL's
        // string syntax is the same. Hand-rolling this is what #719 reports.
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => value.to_string(),
    }
}

#[cfg(test)]
#[path = "value_json_tests.rs"]
mod value_json_tests;
