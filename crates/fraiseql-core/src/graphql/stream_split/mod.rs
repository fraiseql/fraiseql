//! Nested `@stream`: splitting an already-resolved list into delivery chunks.
//!
//! # Why nested `@stream` is a delivery split and root `@stream` is not
//!
//! The two look like one feature and are two, because the thing they can page over
//! is different.
//!
//! A **root** `@stream` names the query's own list. FraiseQL compiles that query to
//! one SQL statement with `LIMIT`/`OFFSET` arguments it already accepts, so a batch
//! is a real re-execution against the database: the server never holds more than one
//! batch, and the delivery can be resumed from a row offset because the offset means
//! something to the query.
//!
//! A **nested** `@stream` names a list *inside* a row — `users { posts @stream }`.
//! That list is a JSONB array produced by the same single statement as the row it
//! belongs to. There is no per-path pagination to push down: the array is already in
//! memory by the time anything could page it, and asking the database for "the next
//! 10 posts of user 3" would be a second statement, a second snapshot, and an
//! alignment problem with no sound answer (see [`crate::graphql::defer`] for the same
//! argument in full).
//!
//! So this module does what `@defer` does — splits the **delivery** of a result the
//! server already holds. The honest consequences, stated rather than left to be
//! inferred:
//!
//! - it does **not** reduce database work, and does not bound server memory: the whole array was
//!   materialised before the split;
//! - it is always correctly aligned, because there is only one snapshot;
//! - it is **not** resumable — the chunk boundaries are positions in a value that no longer exists
//!   once the response has been delivered, unlike a root `@stream`'s row offset, which is an
//!   argument the query accepts.
//!
//! What it *is* good for is the same thing `@defer` is good for: a client that
//! renders a large nested list incrementally gets its first rows without waiting for
//! the serialisation of the last.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::graphql::{
    types::{Directive, FieldSelection},
    value_json,
};

/// One chunk of a streamed nested list, ready to become an `incremental` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamedChunk {
    /// Response path of the **first item in this chunk**, as the GraphQL
    /// incremental-delivery spec renders a `@stream` payload path: the path of the
    /// list, with the item's index appended.
    pub path:  Vec<Value>,
    /// The `@stream(label:)` argument, when the client supplied one.
    pub label: Option<String>,
    /// The items this chunk delivers, in list order.
    pub items: Vec<Value>,
}

/// A `@stream` on a field whose value cannot be streamed.
///
/// Refused rather than ignored: a directive that silently does nothing on a
/// transport that negotiated incremental delivery reads to the client as
/// "streaming worked", which is the one outcome worse than an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotStreamable {
    /// Response key of the offending field.
    pub field:  String,
    /// What it was instead of a list.
    pub reason: &'static str,
}

/// Whether any **nested** selection carries an enabled `@stream`.
///
/// Root-level `@stream` is excluded: that is real database paging and is planned
/// elsewhere. Cheap pre-scan, so a document without one does not pay for the split.
#[must_use]
#[allow(clippy::implicit_hasher)]
// Reason: the variables map is built by the runtime with the default hasher; a generic
// parameter here would leak into every caller for no benefit (same treatment as
// `defer::contains_defer`).
pub fn contains_nested_stream(
    selections: &[FieldSelection],
    variables: &HashMap<String, Value>,
) -> bool {
    selections.iter().any(|s| contains_stream(&s.nested_fields, variables))
}

/// Whether any selection in `selections` — at this level or below — carries an
/// enabled `@stream`.
fn contains_stream(selections: &[FieldSelection], variables: &HashMap<String, Value>) -> bool {
    selections.iter().any(|s| {
        stream_directive(s, variables).is_some_and(|d| d.enabled)
            || contains_stream(&s.nested_fields, variables)
    })
}

/// Split every `@stream`-marked nested list in `data` into its initial slice and
/// its continuation chunks.
///
/// `selections` must be the **effective** selection set — spreads expanded and
/// `@skip`/`@include` applied — so a field the client never asked for cannot be
/// streamed into existence. `data` is mutated in place: each streamed list is
/// truncated to its `initialCount`, and the removed tail becomes the returned
/// chunks, in document order.
///
/// `batch_size` is the maximum items per chunk and must be at least 1; `0` is
/// treated as `1` rather than producing an infinite number of empty chunks.
///
/// # Errors
///
/// Returns [`NotStreamable`] if a `@stream` marks a field that resolved to
/// something other than a list.
#[allow(clippy::implicit_hasher)] // Reason: as `contains_nested_stream`.
pub fn split(
    selections: &[FieldSelection],
    data: &mut Value,
    variables: &HashMap<String, Value>,
    batch_size: usize,
) -> Result<Vec<StreamedChunk>, NotStreamable> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    walk(selections, data, &mut path, variables, batch_size.max(1), &mut out)?;
    Ok(out)
}

/// A resolved `@stream` directive.
struct StreamSpec {
    /// `@stream(if:)`, defaulting to true.
    enabled:       bool,
    /// `@stream(initialCount:)`, defaulting to 0 — how many items stay in the
    /// immediate payload.
    initial_count: usize,
    /// `@stream(label:)`, when supplied.
    label:         Option<String>,
}

/// Resolve the first `@stream` on a selection.
///
/// The first is the outermost when a spread contributed the field, matching
/// `@defer`'s rule: the outermost directive decides.
fn stream_directive(
    selection: &FieldSelection,
    variables: &HashMap<String, Value>,
) -> Option<StreamSpec> {
    let directive = selection.directives.iter().find(|d| d.name == "stream")?;
    Some(StreamSpec {
        enabled:       directive_arg(directive, "if", variables)
            .is_none_or(|v| v.as_bool().unwrap_or(true)),
        initial_count: directive_arg(directive, "initialCount", variables)
            .and_then(|v| v.as_u64())
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0),
        label:         directive_arg(directive, "label", variables)
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
    batch_size: usize,
    out: &mut Vec<StreamedChunk>,
) -> Result<(), NotStreamable> {
    // A list: the same selection set applies to every element, and each element is a
    // distinct path — without this, a streamed list inside a list would be addressed
    // by the outer list's path and every element would overwrite the last.
    if let Value::Array(items) = value {
        for (index, item) in items.iter_mut().enumerate() {
            path.push(Value::from(index));
            walk(selections, item, path, variables, batch_size, out)?;
            path.pop();
        }
        return Ok(());
    }

    let Value::Object(object) = value else {
        return Ok(());
    };

    for selection in selections {
        let key = selection.response_key();
        match stream_directive(selection, variables) {
            Some(spec) if spec.enabled => {
                let Some(field) = object.get_mut(key) else {
                    // A field that resolved to nothing has nothing to deliver later
                    // either — the same rule `@defer` applies.
                    continue;
                };
                // A `null` list is a resolved absence, not a mistyped selection: the
                // client asked to stream a field the row does not have.
                if field.is_null() {
                    continue;
                }
                let Value::Array(items) = field else {
                    return Err(NotStreamable {
                        field:  key.to_owned(),
                        reason: "@stream requires a list field; this one resolved to a \
                                 single value",
                    });
                };
                path.push(Value::from(key));
                take_tail(items, &spec, path, batch_size, out);
                path.pop();
            },
            _ => {
                if selection.nested_fields.is_empty() {
                    continue;
                }
                if let Some(child) = object.get_mut(key) {
                    path.push(Value::from(key));
                    let result =
                        walk(&selection.nested_fields, child, path, variables, batch_size, out);
                    path.pop();
                    result?;
                }
            },
        }
    }
    Ok(())
}

/// Truncate `items` to `initial_count` and turn the removed tail into chunks.
///
/// `path` is the list's own response path; each chunk's path is that plus the index
/// of its first item, which is what a client uses to splice the chunk back into the
/// list it is rendering.
fn take_tail(
    items: &mut Vec<Value>,
    spec: &StreamSpec,
    path: &[Value],
    batch_size: usize,
    out: &mut Vec<StreamedChunk>,
) {
    if items.len() <= spec.initial_count {
        return;
    }
    let tail = items.split_off(spec.initial_count);
    for (offset, chunk) in tail.chunks(batch_size).enumerate() {
        let start = spec.initial_count + offset * batch_size;
        let mut chunk_path = path.to_vec();
        chunk_path.push(Value::from(start));
        out.push(StreamedChunk {
            path:  chunk_path,
            label: spec.label.clone(),
            items: chunk.to_vec(),
        });
    }
}

/// Render a chunk as the `incremental` entry an incremental transport delivers.
#[must_use]
pub fn incremental_entry(chunk: StreamedChunk) -> Value {
    let mut entry = Map::new();
    entry.insert("items".to_owned(), Value::Array(chunk.items));
    entry.insert("path".to_owned(), Value::Array(chunk.path));
    if let Some(label) = chunk.label {
        entry.insert("label".to_owned(), Value::String(label));
    }
    Value::Object(entry)
}
