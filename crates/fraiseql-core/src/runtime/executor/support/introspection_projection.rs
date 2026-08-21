//! Projecting the pre-built introspection response onto a client's selection set.
//!
//! GraphQL § 6.3: a response follows the selection set. `__schema` and `__type`
//! did not — they returned a response built once at startup, so
//! `{ __schema { queryType { name } } }` came back with `description`,
//! `directives`, `queryType` **and** `types`, and `{ __schema { types { name } } }`
//! returned every type with `description`, `fields`, `interfaces` and `kind`.
//!
//! # Why this does not cost what it looks like it costs
//!
//! The canned response existed for a reason — it is large by construction and
//! requested on every `GraphiQL` page load — and projecting it per request would
//! trade that away. But the trade is not forced: **projection is a pure function
//! of the selection set**, and the space of introspection selection sets seen in
//! the wild is tiny and repetitive. `GraphiQL` sends one canonical query, Apollo
//! sends one, each codegen tool sends one, and they do not vary between page
//! loads. Hashing the normalised selection set and memoising the projected value
//! makes it a table lookup again after the first hit per shape, so the zero-cost
//! property survives and the spec deviation does not.
//!
//! # Why the deviation was worth removing
//!
//! Over-delivery is harmless only if every consumer tolerates unknown fields. A
//! client deserialising into a strict typed structure, or tooling that *diffs*
//! introspection results, gets a real failure. More structurally: a pre-built
//! blob makes field- or type-level introspection filtering — hiding internal
//! types, or a partial-introspection mode for semi-trusted clients —
//! **impossible**, because the filter has nowhere to live.
//!
//! # What this does not do
//!
//! * **It does not rebuild the response.** The pre-built value stays the source; this projects it
//!   on the way out.
//! * **It does not invent fields.** A selection naming something the response does not carry is
//!   omitted rather than emitted as `null` — the introspection value is the authority on what
//!   exists, and fabricating a key would be the silent-wrong-answer pattern this whole family is
//!   about.

use std::hash::{Hash, Hasher};

use crate::graphql::FieldSelection;

/// A stable hash of the *shape* a selection set projects to.
///
/// Aliases are part of the shape (they name output keys), so they are hashed;
/// arguments and directives are not, because they do not affect which keys the
/// projection emits. Order is included: two orderings produce different response
/// key orders, so they are genuinely different projections.
#[must_use]
pub fn selection_shape_hash(root: &str, selections: &[FieldSelection]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    hash_selections(selections, &mut hasher);
    hasher.finish()
}

fn hash_selections(selections: &[FieldSelection], hasher: &mut impl Hasher) {
    selections.len().hash(hasher);
    for sel in selections {
        sel.name.hash(hasher);
        sel.alias.hash(hasher);
        hash_selections(&sel.nested_fields, hasher);
    }
}

/// Project `value` onto `selections`.
///
/// Objects keep only the selected keys, under their response key (alias when
/// present). Arrays are projected element-wise — a selection set applies to
/// every member of a list. Scalars are returned whole: a selection set cannot
/// narrow a leaf.
///
/// An empty selection set means the caller asked for the value itself, so it is
/// returned unchanged.
#[must_use]
pub fn project(value: &serde_json::Value, selections: &[FieldSelection]) -> serde_json::Value {
    if selections.is_empty() {
        return value.clone();
    }

    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|i| project(i, selections)).collect())
        },
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(selections.len());
            for sel in selections {
                // `__typename` on an introspection type is answered by the
                // introspection value itself when present; there is no schema
                // type to synthesise it from here.
                let Some(child) = map.get(sel.name.as_str()) else {
                    continue;
                };
                out.insert(sel.response_key().to_string(), project(child, &sel.nested_fields));
            }
            serde_json::Value::Object(out)
        },
        // A leaf: `{ name }` on a string is just the string.
        other => other.clone(),
    }
}

/// Project a full introspection response envelope (`{"data":{"__schema":…}}`)
/// onto the selections written under `root_field`.
///
/// Returns the envelope with the root field's value projected, leaving the
/// `data` wrapper intact.
#[must_use]
pub fn project_response(
    response: &serde_json::Value,
    root_field: &str,
    selections: &[FieldSelection],
) -> serde_json::Value {
    let Some(inner) = response.get("data").and_then(|d| d.get(root_field)) else {
        return response.clone();
    };
    serde_json::json!({ "data": { root_field: project(inner, selections) } })
}

#[cfg(test)]
#[path = "introspection_projection_tests.rs"]
mod tests;
