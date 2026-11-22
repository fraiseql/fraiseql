//! Editing a configuration file's generated JSON Schema: the keys a loader refuses by name.
//!
//! A configuration file's schema is generated from the type that reads it, so a key the type
//! refuses only by name (a retired spelling, another file's spelling) is absent from it,
//! and an editor reports it as unknown, without the loader's words. Each such key is
//! written into the schema as `"not": {}`, so an editor flags it, with `"deprecated": true`
//! and the loader's own refusal as its description.

use serde_json::Value;

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

#[cfg(test)]
mod tests;
