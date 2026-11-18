//! The JSON Schema of `fraiseql.toml`, generated from [`TomlSchema`](super::TomlSchema), and
//! the walk that finds a key it does not take.
//!
//! The schema is the grammar's second statement, so it is generated from the type that reads
//! the file rather than written beside it. The walk reads a parsed file against it and names
//! the first key no table takes: where it was written, and every key that table does take.
//! It is a second reader of the file, so it refuses only where the schema says a table is
//! closed; a shape it cannot judge is left to serde, which refuses it in prose.

use std::sync::OnceLock;

use serde_json::Value;

/// The JSON Schema an editor checks `fraiseql.toml` against.
#[must_use]
pub fn toml_json_schema() -> Value {
    schema().clone()
}

fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::to_value(schemars::schema_for!(super::TomlSchema)).unwrap_or_default()
    })
}

/// A key the file's grammar does not take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKey {
    /// The table it was written in, dotted (`caching.rules`); empty at the top level.
    pub table:    String,
    /// The key as written.
    pub key:      String,
    /// Every key that table takes.
    pub accepted: Vec<String>,
}

/// The first key in `file` that no table of `fraiseql.toml` takes, if any.
#[must_use]
pub fn unknown_key(file: &toml::Table) -> Option<UnknownKey> {
    let root = schema();
    walk_table(root, root, file, &mut Vec::new())
}

/// `node` with every `$ref` followed.
fn follow<'a>(root: &'a Value, mut node: &'a Value) -> &'a Value {
    // A cycle of references would be a schemars bug; the bound keeps it from hanging.
    for _ in 0..64 {
        let Some(name) = node
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix("#/$defs/"))
        else {
            return node;
        };
        node = &root["$defs"][name];
    }
    node
}

fn walk(
    root: &Value,
    node: &Value,
    value: &toml::Value,
    path: &mut Vec<String>,
) -> Option<UnknownKey> {
    let node = follow(root, node);
    for combinator in ["anyOf", "oneOf"] {
        if let Some(arms) = node.get(combinator).and_then(Value::as_array) {
            return walk_arms(root, arms, value, path);
        }
    }
    if node.get("allOf").is_some() {
        // Not a shape this grammar generates today; serde judges it.
        return None;
    }
    match value {
        toml::Value::Table(table) => walk_table(root, node, table, path),
        toml::Value::Array(items) => {
            let item_schema = node.get("items")?;
            items.iter().find_map(|item| walk(root, item_schema, item, path))
        },
        _ => None,
    }
}

/// A value under `anyOf` / `oneOf` is refused only when every arm that could hold it refuses
/// it; the refusal then names every key any of those arms takes.
///
/// An arm of an internally tagged enum names its tag as a `const` (`source = "jwt"`). A table
/// writing another tag cannot be that arm, as serde reads it, so the arm is not consulted:
/// otherwise its refusal of the written arm's own key (`header`, under `source = "header"`)
/// would be reported instead of the key no arm takes. A tag no arm writes is left to serde.
fn walk_arms(
    root: &Value,
    arms: &[Value],
    value: &toml::Value,
    path: &mut Vec<String>,
) -> Option<UnknownKey> {
    let arms = arms
        .iter()
        .filter(|arm| follow(root, arm).get("type").and_then(Value::as_str) != Some("null"))
        .filter(|arm| tag_admits(root, arm, value));
    let mut refused: Option<UnknownKey> = None;
    for arm in arms {
        let unknown = walk(root, arm, value, path)?;
        match &mut refused {
            None => refused = Some(unknown),
            Some(first) => {
                for key in unknown.accepted {
                    if !first.accepted.contains(&key) {
                        first.accepted.push(key);
                    }
                }
            },
        }
    }
    refused
}

/// Whether `value` could be `arm`: false when the arm's tag (a property with a string
/// `const`) is written in `value` as another string.
fn tag_admits(root: &Value, arm: &Value, value: &toml::Value) -> bool {
    let (Some(properties), toml::Value::Table(table)) =
        (follow(root, arm).get("properties").and_then(Value::as_object), value)
    else {
        return true;
    };
    properties
        .iter()
        .all(|(key, schema)| match (schema.get("const"), table.get(key)) {
            (Some(Value::String(tag)), Some(toml::Value::String(written))) => tag == written,
            _ => true,
        })
}

fn walk_table(
    root: &Value,
    node: &Value,
    table: &toml::Table,
    path: &mut Vec<String>,
) -> Option<UnknownKey> {
    let node = follow(root, node);
    let properties = node.get("properties").and_then(Value::as_object);
    let additional = node.get("additionalProperties");
    for (key, value) in table {
        let schema = match (properties.and_then(|p| p.get(key)), additional) {
            (Some(schema), _) | (None, Some(schema @ Value::Object(_))) => schema,
            (None, Some(Value::Bool(false))) => {
                return Some(UnknownKey {
                    table:    path.join("."),
                    key:      key.clone(),
                    accepted: properties.map(|p| p.keys().cloned().collect()).unwrap_or_default(),
                });
            },
            // An open table, or one this walk cannot judge: serde reads it.
            (None, _) => continue,
        };
        path.push(key.clone());
        let unknown = walk(root, schema, value, path);
        path.pop();
        if unknown.is_some() {
            return unknown;
        }
    }
    None
}

#[cfg(test)]
mod tests;
