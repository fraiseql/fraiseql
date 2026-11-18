//! The JSON Schema of `fraiseql.toml`, generated from [`TomlSchema`](super::TomlSchema), and
//! the walk that finds a key it does not take.
//!
//! The schema is the grammar's second statement, so it is generated from the type that reads
//! the file rather than written beside it. The walk reads a parsed file against it and names
//! the first key no table takes: where it was written, and every key that table does take.
//! It is a second reader of the file, so it refuses only where the schema says a table is
//! closed; a shape it cannot judge is left to serde, which refuses it in prose.
//!
//! A key the compiler refuses by name in every role the file plays (a retired spelling, a
//! key never applied, a spelling of the server's file) is in the schema as `"not": {}`, so an
//! editor flags it, with `"deprecated": true` and the compiler's own refusal as its
//! description.

use std::sync::OnceLock;

use serde_json::Value;

/// Where the schema is committed, from the repository root.
pub const TOML_JSON_SCHEMA_PATH: &str = "docs/schemas/fraiseql.toml.schema.json";

/// The schema's URL for this release: the committed file at the release's tag, so a file
/// pointing at it is checked against the grammar of the compiler it was written for. It is
/// the schema's `$id`.
#[must_use]
pub fn toml_json_schema_url() -> String {
    format!(
        "https://raw.githubusercontent.com/fraiseql/fraiseql/v{}/{TOML_JSON_SCHEMA_PATH}",
        env!("CARGO_PKG_VERSION")
    )
}

/// The JSON Schema an editor checks `fraiseql.toml` against.
#[must_use]
pub fn toml_json_schema() -> Value {
    schema().clone()
}

fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let mut schema =
            serde_json::to_value(schemars::schema_for!(super::TomlSchema)).unwrap_or_default();
        for refused in crate::config::refused_keys() {
            mark_refused(&mut schema, &refused);
        }
        schema["$id"] = Value::String(toml_json_schema_url());
        schema["title"] = Value::String("fraiseql.toml".to_owned());
        schema["description"] = Value::String(
            "The configuration the FraiseQL compiler reads: a project configuration beside a \
             schema document, or a schema authored in TOML."
                .to_owned(),
        );
        schema
    })
}

/// Whether `node` is a key the compiler refuses by name.
fn is_refused(node: &Value) -> bool {
    node.get("not").is_some()
}

/// The JSON pointer of the object schema `pointer` stands for: every `$ref` followed, and an
/// `Option`'s value arm taken.
fn object_at(root: &Value, mut pointer: String) -> String {
    for _ in 0..64 {
        let Some(node) = root.pointer(&pointer) else {
            return pointer;
        };
        if let Some(name) = node
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix("#/$defs/"))
        {
            pointer = format!("/$defs/{}", escape(name));
            continue;
        }
        let arm = ["anyOf", "oneOf"].into_iter().find_map(|combinator| {
            let arms = node.get(combinator)?.as_array()?;
            let index = arms
                .iter()
                .position(|arm| arm.get("type").and_then(Value::as_str) != Some("null"))?;
            Some(format!("{pointer}/{combinator}/{index}"))
        });
        match arm {
            Some(arm) => pointer = arm,
            None => return pointer,
        }
    }
    pointer
}

fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// The property `name` of the object at `pointer`, made when the schema has none.
fn property(root: &mut Value, pointer: &str, name: &str) -> String {
    let object = object_at(root, pointer.to_owned());
    if let Some(Value::Object(node)) = root.pointer_mut(&object) {
        let properties = node
            .entry("properties")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Value::Object(properties) = properties {
            properties.entry(name).or_insert_with(|| Value::Object(serde_json::Map::new()));
        }
    }
    format!("{object}/properties/{}", escape(name))
}

/// `refused` written into `schema`: any value refused, deprecated, described by its refusal.
/// A refused table keeps the refused keys under it, so each is still described.
fn mark_refused(schema: &mut Value, refused: &crate::config::RefusedKey) {
    let mut pointer = String::new();
    for part in refused.section.split('.').filter(|part| !part.is_empty()) {
        pointer = property(schema, &pointer, part);
    }
    if !refused.key.is_empty() {
        pointer = property(schema, &pointer, refused.key);
    }
    let kept = schema
        .pointer(&pointer)
        .and_then(|node| node.get("properties"))
        .filter(|_| refused.key.is_empty())
        .cloned();
    let mut node = serde_json::json!({
        "not": {},
        "deprecated": true,
        "description": refused.message,
    });
    if let Some(kept) = kept {
        node["properties"] = kept;
    }
    if let Some(target) = schema.pointer_mut(&pointer) {
        *target = node;
    }
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
                    accepted: properties
                        .map(|p| {
                            p.iter()
                                .filter(|(_, schema)| !is_refused(schema))
                                .map(|(key, _)| key.clone())
                                .collect()
                        })
                        .unwrap_or_default(),
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
