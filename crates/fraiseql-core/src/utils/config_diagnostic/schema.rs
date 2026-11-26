//! A configuration file's generated JSON Schema: the keys a loader refuses by name written
//! into it, and the walk that finds a key it does not take.
//!
//! A configuration file's schema is generated from the type that reads it, so a key the type
//! refuses only by name (a retired spelling, another file's spelling) is absent from it,
//! and an editor reports it as unknown, without the loader's words. Each such key is
//! written into the schema as `"not": {}`, so an editor flags it, with `"deprecated": true`
//! and the loader's own refusal as its description.

use serde_json::Value;

use super::{ConfigDiagnostic, DiagnosticCode};

/// Whether `node` is a key a loader refuses by name.
#[must_use]
pub fn is_refused(node: &Value) -> bool {
    node.get("not").is_some()
}

/// The JSON pointer of the object schema `pointer` stands for: every `$ref` followed, and an
/// `Option`'s value arm taken.
fn object_at(root: &Value, mut pointer: String) -> String {
    // A cycle of references would be a schemars bug; the bound keeps it from hanging.
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

/// Write `key` of the dotted `section` into `schema` as refused, with `message`.
///
/// With `key` empty the section itself is refused. Any value is refused, the key is marked
/// deprecated and described by `message`. A refused table keeps the keys under it, so each
/// is still described.
pub fn mark_refused(schema: &mut Value, section: &str, key: &str, message: &str) {
    let mut pointer = String::new();
    for part in section.split('.').filter(|part| !part.is_empty()) {
        pointer = property(schema, &pointer, part);
    }
    if !key.is_empty() {
        pointer = property(schema, &pointer, key);
    }
    let kept = schema
        .pointer(&pointer)
        .and_then(|node| node.get("properties"))
        .filter(|_| key.is_empty())
        .cloned();
    let mut node = serde_json::json!({
        "not": {},
        "deprecated": true,
        "description": message,
    });
    if let Some(kept) = kept {
        node["properties"] = kept;
    }
    if let Some(target) = schema.pointer_mut(&pointer) {
        *target = node;
    }
}

/// Set `name` to `value` on the schema of the dotted `path`'s key, through every `$ref` and
/// `Option` on the way. A key the schema does not describe is left undescribed: an
/// annotation never adds a key.
pub fn annotate(schema: &mut Value, path: &str, name: &str, value: Value) {
    let mut pointer = String::new();
    for part in path.split('.').filter(|part| !part.is_empty()) {
        let object = object_at(schema, pointer);
        let exists = schema
            .pointer(&object)
            .and_then(|node| node.get("properties"))
            .and_then(|properties| properties.get(part))
            .is_some();
        if !exists {
            return;
        }
        pointer = format!("{object}/properties/{}", escape(part));
    }
    if let Some(Value::Object(node)) = schema.pointer_mut(&pointer) {
        node.insert(name.to_owned(), value);
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

/// The first key in `file` that no table of `schema` takes, if any.
///
/// The walk is a second reader of the file, so it refuses only where the schema says a table
/// is closed; a shape it cannot judge is left to serde, which refuses it in prose.
#[must_use]
pub fn unknown_key(schema: &Value, file: &toml::Table) -> Option<UnknownKey> {
    walk_table(schema, schema, file, &mut Vec::new())
}

/// The refusal of `unknown`, written in the file `file_name` names: the accepted spelling
/// nearest to it as the replacement, when one is near, and otherwise every key its table
/// takes.
#[must_use]
pub fn unknown_key_diagnostic(file_name: &str, unknown: &UnknownKey) -> ConfigDiagnostic {
    let accepted: Vec<&str> = unknown.accepted.iter().map(String::as_str).collect();
    let near = crate::runtime::suggest_similar(&unknown.key, &accepted).first().copied();
    let spelling = if unknown.table.is_empty() {
        format!("`{}`", unknown.key)
    } else {
        format!("`[{}] {}`", unknown.table, unknown.key)
    };
    let message = match near {
        Some(near) => format!("{file_name} takes no {spelling}; did you mean `{near}`?"),
        None => format!(
            "{file_name} takes no {spelling}; remove it. That table takes: {}",
            accepted.join(", ")
        ),
    };
    let diagnostic =
        ConfigDiagnostic::new(DiagnosticCode::UnknownKey, &unknown.table, &unknown.key, message);
    match near {
        Some(near) => diagnostic.with_replacement(&format!("`{near}`")),
        None => diagnostic,
    }
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
                    accepted: properties.map_or_default(|p| {
                        p.iter()
                            .filter(|(_, schema)| !is_refused(schema))
                            .map(|(key, _)| key.clone())
                            .collect()
                    }),
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
