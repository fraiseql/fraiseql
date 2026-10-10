//! A cascade derived from `pg_tviews`' affected set (#1391).
//!
//! A mutation compiled with `cascade_source = "pg_tviews"` asks the adapter, inside its
//! transaction and before the gate, for the TVIEW rows the transaction changed
//! (`tviews.pg_tviews_flush_and_report()`), each updated row read from its type's view on
//! the write's own connection: row visibility is the database's, as for the rows the
//! function's own `cascade_entity` reads. This module merges that report into the
//! function's `cascade` before the row is adjudicated, so every derived entry is then served
//! (and field-authorized) as the function's own entries are.
//!
//! - The function's entry for a row wins over the report's.
//! - A reported row the caller's view did not return, a type no view serves, and a type `pg_tviews`
//!   truncated are not listed: the type is invalidated instead, one hint per root query returning
//!   it. A truncated type lists none of its rows, never a partial set.

use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap, HashSet},
};

use serde_json::{Map, Value, json};

use crate::schema::CompiledSchema;

/// The histogram of entries a derived cascade served, on the server's `/metrics`.
pub(super) const DERIVED_ENTRIES: &str = "fraiseql_cascade_derived_entries";

/// The types a derived cascade can read, with the view each is read from: every queryable
/// type (a source, not an error type).
pub(super) fn views(schema: &CompiledSchema) -> Vec<(String, String)> {
    schema
        .types
        .iter()
        .filter(|t| !t.is_error && !t.sql_source.as_str().is_empty())
        .map(|t| (t.name.to_string(), t.sql_source.as_str().to_string()))
        .collect()
}

fn key(entry: &Value) -> (String, String) {
    let id = match &entry["id"] {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    (entry["__typename"].as_str().unwrap_or_default().to_string(), id)
}

fn entries<'a>(cascade: &'a Map<String, Value>, arm: &str) -> &'a [Value] {
    cascade.get(arm).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// `row` with the derived report it carries merged into its `cascade`; `row` itself when
/// it carries none.
pub(super) fn merge<'r>(
    schema: &CompiledSchema,
    row: &'r HashMap<String, Value>,
) -> Cow<'r, HashMap<String, Value>> {
    let Some(report) = row.get(crate::backend::DERIVED_CASCADE_KEY) else {
        return Cow::Borrowed(row);
    };
    let mut merged = row.clone();
    merged.remove(crate::backend::DERIVED_CASCADE_KEY);
    let mut cascade = match merged.remove("cascade") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };

    let own: HashSet<(String, String)> = entries(&cascade, "updated")
        .iter()
        .chain(entries(&cascade, "deleted"))
        .map(key)
        .collect();
    let truncated: BTreeSet<String> = report["invalidated_types"]
        .as_array()
        .map(|types| types.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let mut invalidated = truncated.clone();

    let mut derived_updated = Vec::new();
    for entry in report["updated"].as_array().map_or(&[][..], Vec::as_slice) {
        let entry_key = key(entry);
        if own.contains(&entry_key) || truncated.contains(&entry_key.0) {
            continue;
        }
        if entry["entity"].is_null() {
            invalidated.insert(entry_key.0);
            continue;
        }
        derived_updated.push(json!({
            "__typename": entry["__typename"],
            "id": entry["id"],
            "operation": entry["operation"],
            "entity": entry["entity"],
        }));
    }
    let derived_deleted: Vec<Value> = report["deleted"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|entry| {
            let entry_key = key(entry);
            !own.contains(&entry_key) && !truncated.contains(&entry_key.0)
        })
        .cloned()
        .collect();

    // The fan-out a derived cascade served, for sizing `max_updated_entities` (#1391).
    #[allow(clippy::cast_precision_loss)]
    // Reason: a count of cascade entries, far below f64's exact-integer range.
    metrics::histogram!(DERIVED_ENTRIES)
        .record((derived_updated.len() + derived_deleted.len()) as f64);

    for (arm, derived) in [("updated", derived_updated), ("deleted", derived_deleted)] {
        if derived.is_empty() {
            continue;
        }
        let mut list = entries(&cascade, arm).to_vec();
        list.extend(derived);
        cascade.insert(arm.to_string(), Value::Array(list));
    }

    if !invalidated.is_empty() {
        let mut hints = entries(&cascade, "invalidations").to_vec();
        for type_name in &invalidated {
            for query in schema.queries.iter().filter(|q| q.return_type == *type_name) {
                let hint = json!({
                    "queryName": query.name,
                    "strategy": "INVALIDATE",
                    "scope": "PREFIX",
                });
                if !hints.contains(&hint) {
                    hints.push(hint);
                }
            }
        }
        cascade.insert("invalidations".to_string(), Value::Array(hints));
    }

    merged.insert("cascade".to_string(), Value::Object(cascade));
    Cow::Owned(merged)
}
