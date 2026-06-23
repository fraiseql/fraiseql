//! Multi-root query pipelining — parallel execution of independent query roots.
//!
//! Dispatches multi-root GraphQL queries concurrently using
//! [`futures::future::try_join_all`], then merges the results into a single
//! `{ "data": { ... } }` envelope.
//!
//! # Example
//!
//! ```text
//! { users { id name } posts { id title } }
//! ```
//!
//! Without pipelining: `t_users + t_posts` latency (sequential).
//! With pipelining:    `max(t_users, t_posts)` latency (concurrent).

use std::sync::atomic::{AtomicU64, Ordering};

use super::super::{Executor, root_type_name};
use crate::{
    db::traits::DatabaseAdapter,
    error::Result,
    graphql::{FieldSelection, GraphQLArgument, ParsedQuery},
};

// ── Prometheus counter ────────────────────────────────────────────────────────

static MULTI_ROOT_QUERIES_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Total multi-root GraphQL queries dispatched via the parallel execution path.
pub fn multi_root_queries_total() -> u64 {
    MULTI_ROOT_QUERIES_TOTAL.load(Ordering::Relaxed)
}

// ── Result types ──────────────────────────────────────────────────────────────

/// Result for a single root field in a pipelined execution.
#[derive(Debug)]
pub struct RootFieldResult {
    /// Response key for this field (alias if provided, otherwise field name).
    pub field_name: String,
    /// Resolved data value.
    pub data:       serde_json::Value,
}

/// Aggregated result from a multi-root parallel execution.
#[derive(Debug)]
pub struct PipelineResult {
    /// Results for each root field, in the order they were requested.
    pub fields:   Vec<RootFieldResult>,
    /// `true` when results were produced by the parallel path.
    pub parallel: bool,
}

impl PipelineResult {
    /// Merge all field results into a single JSON map.
    ///
    /// Returns a `serde_json::Map` suitable for embedding in a `"data"` envelope.
    #[must_use]
    pub fn merge_into_data_map(&self) -> serde_json::Map<String, serde_json::Value> {
        self.fields.iter().map(|f| (f.field_name.clone(), f.data.clone())).collect()
    }
}

// ── Detection helpers ─────────────────────────────────────────────────────────

/// Returns `true` when the query has more than one root field selection.
///
/// Only applies to anonymous queries and `query { ... }` operations; mutations
/// and subscriptions are not affected.
#[must_use]
pub const fn is_multi_root(parsed: &ParsedQuery) -> bool {
    parsed.selections.len() > 1
}

/// Returns the response key (alias or field name) for every root-level selection.
///
/// Returns an iterator borrowing from `parsed` so callers that immediately
/// iterate avoid the intermediate `Vec` allocation.
pub fn extract_root_field_names(parsed: &ParsedQuery) -> impl Iterator<Item = &str> + '_ {
    parsed.selections.iter().map(|s| s.response_key())
}

// ── Query-string serializer ───────────────────────────────────────────────────

/// Serialize a root `FieldSelection` to a valid GraphQL query string.
///
/// Produces `{ fieldName(arg: value) { sub1 sub2 { ... } } }`.
/// Variables are preserved as `$varName` references; inline values are
/// converted from their stored JSON representation to GraphQL syntax.
pub(super) fn field_selection_to_query(field: &FieldSelection) -> String {
    format!("{{ {} }}", serialize_field(field))
}

fn serialize_field(field: &FieldSelection) -> String {
    let mut s = String::new();

    // Alias prefix
    if let Some(alias) = &field.alias {
        s.push_str(alias);
        s.push_str(": ");
    }
    s.push_str(&field.name);

    // Arguments
    if !field.arguments.is_empty() {
        s.push('(');
        let args: Vec<String> = field.arguments.iter().map(serialize_arg).collect();
        s.push_str(&args.join(", "));
        s.push(')');
    }

    // Nested sub-selections
    if !field.nested_fields.is_empty() {
        s.push_str(" { ");
        let sub: Vec<String> = field.nested_fields.iter().map(serialize_field).collect();
        s.push_str(&sub.join(" "));
        s.push_str(" }");
    }

    s
}

fn serialize_arg(arg: &GraphQLArgument) -> String {
    format!("{}: {}", arg.name, arg_value_to_graphql(arg))
}

/// Convert a stored `GraphQLArgument` back to a GraphQL-syntax value.
fn arg_value_to_graphql(arg: &GraphQLArgument) -> String {
    match arg.value_type.as_str() {
        "variable" => {
            // value_json is stored as a JSON string e.g. `"\"$varName\""`.
            // Parse it to get the raw `$varName`.
            serde_json::from_str::<String>(&arg.value_json)
                .unwrap_or_else(|_| arg.value_json.clone())
        },
        "object" => {
            // JSON objects use quoted keys; GraphQL objects don't.
            serde_json::from_str::<serde_json::Value>(&arg.value_json)
                .map_or_else(|_| arg.value_json.clone(), |v| json_value_to_graphql(&v))
        },
        "enum" => {
            // Strip surrounding JSON quotes from enum values.
            serde_json::from_str::<String>(&arg.value_json)
                .unwrap_or_else(|_| arg.value_json.clone())
        },
        // int, float, boolean, null, string, list — value_json is already valid GraphQL.
        _ => arg.value_json.clone(),
    }
}

/// Recursively convert a `serde_json::Value` to GraphQL value syntax.
fn json_value_to_graphql(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Object(map) => {
            let pairs: Vec<String> =
                map.iter().map(|(k, v)| format!("{k}: {}", json_value_to_graphql(v))).collect();
            format!("{{{}}}", pairs.join(", "))
        },
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_value_to_graphql).collect();
            format!("[{}]", items.join(", "))
        },
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
    }
}

// ── Parallel execution ────────────────────────────────────────────────────────

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute all root fields of a multi-root query concurrently.
    ///
    /// Each root field is dispatched as an independent single-root query.
    /// Results are awaited with [`futures::future::try_join_all`] and merged
    /// into a `PipelineResult`.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered across all concurrent sub-queries.
    pub async fn execute_parallel(
        &self,
        parsed: &ParsedQuery,
        variables: Option<&serde_json::Value>,
        security_context: Option<&crate::security::SecurityContext>,
    ) -> Result<PipelineResult> {
        MULTI_ROOT_QUERIES_TOTAL.fetch_add(1, Ordering::Relaxed);

        // Root `__typename` resolves to the operation's root type name with no DB
        // round-trip (GraphQL spec §"Type Name Introspection"). It is a meta-field
        // available at every selection set, including the root; dispatching it as a
        // regular sub-query would fail `find_query`. We resolve it locally and only
        // dispatch the genuine data-bearing roots.
        //
        // NOTE: `@skip`/`@include` on a root field are not honoured on the
        // multi-root path — `field_selection_to_query` drops directives when it
        // re-serializes every root (a pre-existing limitation for all multi-root
        // fields, not specific to `__typename`). The single-root `TypeName` path
        // does honour them.
        let root_type = root_type_name(&parsed.operation_type);

        // Synthetic single-root query strings for every data-bearing selection,
        // tagged with their original index so results can be reassembled in request
        // order. `__typename` roots are skipped here and resolved below. (Owned —
        // avoids borrow lifetime entanglement between iterations and the final zip.)
        let dispatched: Vec<(usize, String, String)> = parsed
            .selections
            .iter()
            .enumerate()
            .filter(|(_, f)| f.name != "__typename")
            .map(|(i, f)| (i, f.response_key().to_string(), field_selection_to_query(f)))
            .collect();

        // Pre-create one QueryRunner per sub-query (each is a cheap Arc::clone).
        // Storing them in a Vec ensures they live long enough for the futures to borrow from.
        let runners: Vec<_> = dispatched.iter().map(|_| self.query_runner()).collect();

        // Build futures — each borrows from its corresponding runner in `runners`.
        // Both `runners` and `futs` are owned by this function scope, so the borrows are valid.
        let futs: Vec<_> = runners
            .iter()
            .zip(dispatched.iter())
            .map(|(runner, (_, _, query))| {
                runner.execute_regular_query_maybe_security(
                    query.as_str(),
                    variables,
                    security_context,
                )
            })
            .collect();

        // Drive all futures concurrently (single-threaded cooperative multitasking).
        let results = futures::future::try_join_all(futs).await?;

        // Extract the per-field `data` from each `{"data":{"field":[...]}}` response,
        // keyed by the original selection index.
        let mut dispatched_data: std::collections::HashMap<usize, serde_json::Value> = results
            .into_iter()
            .zip(dispatched.iter())
            .map(|(response, (index, key, _))| (*index, response["data"][key.as_str()].clone()))
            .collect();

        // Reassemble in request order: `__typename` roots resolve locally; every
        // other root pulls its dispatched data by index.
        let fields = parsed
            .selections
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let field_name = f.response_key().to_string();
                let data = if f.name == "__typename" {
                    serde_json::Value::String(root_type.to_string())
                } else {
                    dispatched_data.remove(&index).unwrap_or(serde_json::Value::Null)
                };
                RootFieldResult { field_name, data }
            })
            .collect();

        Ok(PipelineResult {
            fields,
            parallel: true,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
