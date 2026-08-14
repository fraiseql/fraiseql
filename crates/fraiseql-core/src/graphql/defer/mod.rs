//! `@defer`: splitting a resolved response into its immediate and deferred parts.
//!
//! # Why this is a response split, not a second query
//!
//! In a resolver-per-field server, `@defer` exists to stop an expensive resolver from
//! holding up the rest of the response. FraiseQL has no per-field resolvers: a query
//! compiles to **one** SQL statement over a JSONB view, and every requested field is
//! produced by that one statement. There are exactly two ways to honour `@defer` here:
//!
//! 1. Remove the deferred fields from the projection and run a **second** statement for them. That
//!    genuinely saves work in the first statement — and for a list it is unsound, because the two
//!    statements are separate snapshots. Aligning the second result to the first positionally
//!    attaches one row's deferred fields to another row whenever a concurrent write shifts the
//!    window, silently. Aligning by key would require the non-deferred selection to carry an
//!    identity field, which the client is under no obligation to request.
//! 2. Run the one statement and split its **delivery**. Always correct — one snapshot, so alignment
//!    is not a question that can be got wrong — and it is what an incrementally-rendering client
//!    actually consumes.
//!
//! FraiseQL does (2). The honest consequence, stated here so no one infers otherwise:
//! **`@defer` does not reduce database work.** It changes when bytes reach the client,
//! which is a real benefit when the deferred part is large (a big nested list) and
//! close to nothing when it is small.
//!
//! Over a buffered transport `@defer` remains a no-op — a server may always deliver
//! the full result — so this splitting runs only on an incremental transport.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::graphql::{
    types::{Directive, FieldSelection},
    value_json,
};

/// One deferred fragment's payload, ready to become an `incremental` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredPayload {
    /// Response path of the object these fields belong to, as the GraphQL
    /// incremental-delivery spec renders it: field keys and list indices.
    /// Empty for fields deferred at the root of `data`.
    pub path:  Vec<Value>,
    /// The `@defer(label:)` argument, when the client supplied one. Clients use it
    /// to route a payload without pattern-matching on the path.
    pub label: Option<String>,
    /// The deferred fields, keyed exactly as they would have appeared in `data`.
    pub data:  Map<String, Value>,
}

/// Whether any selection in the tree carries an enabled `@defer`.
///
/// Cheap pre-scan: a document without `@defer` must not pay for the split, and must
/// not change shape.
#[must_use]
#[allow(clippy::implicit_hasher)]
// Reason: the variables map is built by the runtime with the default hasher; a generic
// parameter here would leak into every caller for no benefit (same treatment as
// `selection_set::filter` and `value_json::resolve_variables`).
pub fn contains_defer(selections: &[FieldSelection], variables: &HashMap<String, Value>) -> bool {
    selections.iter().any(|s| {
        defer_directive(s, variables).is_some_and(|d| d.enabled)
            || contains_defer(&s.nested_fields, variables)
    })
}

/// Remove every `@defer`-marked field from `data` and return them grouped by the
/// object they were removed from.
///
/// `selections` must be the **effective** selection set — spreads expanded and
/// `@skip`/`@include` applied — so that a field the client never asked for cannot be
/// deferred into existence. `data` is the response envelope's `data` object; it is
/// mutated in place into the immediate payload.
///
/// Grouping is by `(path, label)` in first-seen order, so two fields deferred by the
/// same fragment arrive in one payload rather than one each.
///
/// A `@defer` on a field with no matching key in `data` contributes nothing: a field
/// that resolved to nothing has nothing to deliver later either.
#[must_use]
#[allow(clippy::implicit_hasher)] // Reason: as `contains_defer`.
pub fn split(
    selections: &[FieldSelection],
    data: &mut Value,
    variables: &HashMap<String, Value>,
) -> Vec<DeferredPayload> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    walk(selections, data, &mut path, variables, &mut out);
    out
}

/// A resolved `@defer` directive.
struct DeferSpec {
    /// `@defer(if:)`, defaulting to true. A disabled `@defer` is not a defer at all.
    enabled: bool,
    /// `@defer(label:)`, when supplied.
    label:   Option<String>,
}

/// Resolve the first enabled-or-not `@defer` on a selection.
///
/// A field can carry more than one: flattening a spread prepends the spread's
/// directives onto each contributed field, so `...Outer @defer` wrapping
/// `...Inner @defer(label: "inner")` leaves both on the leaf. The first is the
/// outermost, and the outermost is the one that decides — a nested `@defer` cannot
/// un-defer what an enclosing one deferred, exactly as a nested `@include` cannot
/// resurrect a field an enclosing `@skip` withheld.
fn defer_directive(
    selection: &FieldSelection,
    variables: &HashMap<String, Value>,
) -> Option<DeferSpec> {
    let directive = selection.directives.iter().find(|d| d.name == "defer")?;
    Some(DeferSpec {
        enabled: directive_arg(directive, "if", variables)
            .is_none_or(|v| v.as_bool().unwrap_or(true)),
        label:   directive_arg(directive, "label", variables)
            .and_then(|v| v.as_str().map(ToOwned::to_owned)),
    })
}

/// Decode a directive argument, resolving a variable reference if present.
fn directive_arg(
    directive: &Directive,
    name: &str,
    variables: &HashMap<String, Value>,
) -> Option<Value> {
    let arg = directive.arguments.iter().find(|a| a.name == name)?;
    let decoded = value_json::decode(&arg.value_json).ok()?;
    Some(value_json::resolve_variables(decoded, variables))
}

/// Recursive split. `path` is the response path of `value`.
fn walk(
    selections: &[FieldSelection],
    value: &mut Value,
    path: &mut Vec<Value>,
    variables: &HashMap<String, Value>,
    out: &mut Vec<DeferredPayload>,
) {
    // A list: the same selection set applies to every element, and each element is a
    // distinct path. Without this, a deferred field inside a list would be addressed
    // by the list's own path and every element would overwrite the last.
    if let Value::Array(items) = value {
        for (index, item) in items.iter_mut().enumerate() {
            path.push(Value::from(index));
            walk(selections, item, path, variables, out);
            path.pop();
        }
        return;
    }

    let Value::Object(object) = value else {
        return;
    };

    for selection in selections {
        let key = selection.response_key();
        match defer_directive(selection, variables) {
            Some(spec) if spec.enabled => {
                if let Some(field) = object.remove(key) {
                    push_deferred(out, path, spec.label, key, field);
                }
            },
            _ => {
                if selection.nested_fields.is_empty() {
                    continue;
                }
                if let Some(child) = object.get_mut(key) {
                    path.push(Value::from(key));
                    walk(&selection.nested_fields, child, path, variables, out);
                    path.pop();
                }
            },
        }
    }
}

/// Append a deferred field to its `(path, label)` group, creating it if new.
fn push_deferred(
    out: &mut Vec<DeferredPayload>,
    path: &[Value],
    label: Option<String>,
    key: &str,
    field: Value,
) {
    if let Some(existing) = out.iter_mut().find(|p| p.path == path && p.label == label) {
        existing.data.insert(key.to_owned(), field);
        return;
    }
    let mut data = Map::new();
    data.insert(key.to_owned(), field);
    out.push(DeferredPayload {
        path: path.to_vec(),
        label,
        data,
    });
}
