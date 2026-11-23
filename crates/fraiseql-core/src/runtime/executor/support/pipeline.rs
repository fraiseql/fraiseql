//! Multi-root query pipelining — parallel execution of independent query roots.
//!
//! Plans every root of a multi-root GraphQL query, then reads them concurrently and
//! merges the results into one `{ "data": { ... } }` envelope. A root refused while
//! planned refuses the document; a root that fails while read is a field error at its
//! path (GraphQL § 6.4.4).
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
    /// Results for each root field, in the order they were requested. A root that failed
    /// is `null` here.
    pub fields:          Vec<RootFieldResult>,
    /// `true` when results were produced by the parallel path.
    pub parallel:        bool,
    /// The field errors of the roots that failed while read, and those their runners
    /// reported, each at its path.
    pub errors:          Vec<serde_json::Value>,
    /// A root published as non-null failed, so `data` is `null` (§ 6.4.4).
    pub data_nulled:     bool,
    /// The type each `node` root's id named, for completion.
    pub(crate) resolved: crate::runtime::completion::ResolvedTypes,
}

impl PipelineResult {
    /// Merge all field results into a single JSON map.
    ///
    /// Returns a `serde_json::Map` suitable for embedding in a `"data"` envelope.
    #[must_use]
    pub fn merge_into_data_map(&self) -> serde_json::Map<String, serde_json::Value> {
        self.fields.iter().map(|f| (f.field_name.clone(), f.data.clone())).collect()
    }

    /// The response: `data` (or `null` when a non-null root failed), and `errors` when any
    /// root reported one.
    #[must_use]
    pub fn into_response(self) -> serde_json::Value {
        let data = if self.data_nulled {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(self.merge_into_data_map())
        };
        let mut response = serde_json::Map::with_capacity(2);
        response.insert("data".to_string(), data);
        if !self.errors.is_empty() {
            response.insert("errors".to_string(), serde_json::Value::Array(self.errors));
        }
        serde_json::Value::Object(response)
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

    // Directives. `execute_parallel` hands this function an already-resolved
    // selection set, so every `@skip`/`@include` here has been evaluated and the
    // sub-query would reach the same answer re-evaluating it. They are emitted
    // anyway: re-serializing a selection while dropping part of it is what made
    // this path lose directives in the first place, and a directive this
    // function does not recognise must not vanish silently.
    for directive in &field.directives {
        s.push_str(" @");
        s.push_str(&directive.name);
        if !directive.arguments.is_empty() {
            s.push('(');
            let args: Vec<String> = directive.arguments.iter().map(serialize_arg).collect();
            s.push_str(&args.join(", "));
            s.push(')');
        }
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
///
/// One conversion for every value type. The previous version dispatched on
/// `value_type` and re-emitted the raw JSON for anything it did not recognise,
/// so a list of objects reached the parser with quoted keys (#902) and any
/// string containing a quote or a backslash produced invalid GraphQL (#719).
fn arg_value_to_graphql(arg: &GraphQLArgument) -> String {
    crate::graphql::value_json::decode(&arg.value_json).map_or_else(
        // Unreachable in practice — `value_json` is written by the parser — and
        // the malformed value is passed through so the parse error names it
        // rather than a silently different query.
        |_| arg.value_json.clone(),
        |value| match arg.value_type.as_str() {
            // An enum value is a bare GraphQL name, not a quoted string.
            "enum" => value.as_str().map_or_else(|| value.to_string(), ToString::to_string),
            _ => crate::graphql::value_json::to_graphql(&value),
        },
    )
}

// ── Parallel execution ────────────────────────────────────────────────────────

/// One root of a multi-root document, planned by the runner that reads it.
enum PlannedRoot<'a> {
    /// A declared query.
    Regular(Box<super::super::runners::query_regular::PlannedRegular>),
    /// A fact table's aggregate or window root.
    FactTable(Box<super::super::runners::aggregate::PlannedFactRoot<'a>>),
    /// A Relay `node(id:)` lookup.
    Node(Box<super::super::runners::query_relay::PlannedNode>),
}

impl Executor {
    /// Execute all root fields of a multi-root query: plan every root, then read them
    /// concurrently.
    ///
    /// Each root is planned as the single-root document it would be alone: matched, its
    /// operation, field and scope gates checked, its arguments and row scope resolved. Any
    /// root refused there refuses the document before anything is read, as GraphQL
    /// validation does. `node` roots are planned last: a bare UUID's type is found by
    /// reading for it, so every other root's refusal comes first. The planned roots are then read
    /// concurrently, and a root whose read fails is `null` with a field error at its path while
    /// the others are served; when that root is published non-null, `data` is `null` (§ 6.4.4).
    /// Its error is rendered by the embedder's
    /// [`root_error_renderer`](crate::runtime::RuntimeConfig), as a failed multi-root
    /// mutation's is.
    ///
    /// # Errors
    ///
    /// The first root refused while planned.
    pub async fn execute_parallel(
        &self,
        parsed: &ParsedQuery,
        variables: Option<&serde_json::Value>,
        security_context: Option<&crate::security::SecurityContext>,
    ) -> Result<PipelineResult> {
        MULTI_ROOT_QUERIES_TOTAL.fetch_add(1, Ordering::Relaxed);

        // Reduce the document to the fields the client asked for *before* fanning
        // out. Each root is matched as its own synthetic query string, and that string
        // carries neither the document's fragment definitions nor its directives:
        //
        //   * `{ users { ...F } posts { id } } fragment F on User { … }` reached the sub-parse with
        //     `F` undefined and failed the whole request with "Fragment not found".
        //   * `@skip`/`@include`, at the root or nested, were dropped by re-serialization and
        //     silently had no effect.
        //
        // Resolving here means the synthetic strings contain no spreads at all,
        // and every directive has already been decided. A `node` root's selection is checked
        // against `Node` first, while its fragments can still be told from its own fields.
        if self.ctx.schema.publishes_node() {
            for root in parsed.selections.iter().filter(|root| root.name == "node") {
                super::node_selection::check_node_selection(
                    &self.ctx.schema,
                    root,
                    &parsed.fragments,
                )?;
            }
        }
        let resolved = crate::graphql::selection_set::resolve_and_filter(
            &parsed.selections,
            &parsed.fragments,
            &crate::graphql::selection_set::variables_map(variables),
            self.max_query_depth(),
        )?;

        // Root `__typename` resolves to the operation's root type name with no DB
        // round-trip (GraphQL spec §"Type Name Introspection"). It is a meta-field
        // available at every selection set, including the root; it is answered below,
        // and only the data-bearing roots are planned and read.
        let root_type = root_type_name(&parsed.operation_type);
        let runner = self.query_runner();
        let fact_tables = self.aggregate_runner();

        // 1. Plan every data-bearing root, in request order. `None` as the operation name,
        //    deliberately: `field_selection_to_query` re-serialises each root into a *synthetic*
        //    single-root anonymous document, which the request's `operationName` does not name.
        //    Operation selection already happened upstream, in classification.
        let is_node =
            |field: &FieldSelection| field.name == "node" && self.ctx.schema.publishes_node();
        let mut planned = Vec::with_capacity(resolved.len());
        for (index, field) in resolved.iter().enumerate() {
            if field.name == "__typename" || is_node(field) {
                continue;
            }
            // A fact-table root is planned by its own runner, from the root itself: it is
            // no declared query for the matcher to find.
            let plan = if field.name.ends_with("_aggregate") {
                PlannedRoot::FactTable(Box::new(fact_tables.plan_aggregate(
                    &field.name,
                    Some(field),
                    Some(field),
                    variables,
                    security_context,
                )?))
            } else if field.name.ends_with("_window") {
                PlannedRoot::FactTable(Box::new(fact_tables.plan_window(
                    &field.name,
                    Some(field),
                    variables,
                    security_context,
                )?))
            } else {
                let query = field_selection_to_query(field);
                let query_match =
                    self.ctx.matcher.match_query_for(&query, variables, None, security_context)?;
                PlannedRoot::Regular(Box::new(runner.plan_regular_match(
                    query_match,
                    variables,
                    security_context,
                )?))
            };
            planned.push((index, field, plan));
        }
        for (index, field) in resolved.iter().enumerate().filter(|(_, field)| is_node(field)) {
            let selections = super::classify::flatten_node_fragments(field.nested_fields.clone());
            let plan = runner.plan_node(field, &selections, variables, security_context).await?;
            planned.push((index, field, PlannedRoot::Node(Box::new(plan))));
        }

        // 2. Read the planned roots concurrently.
        let reads = planned.into_iter().map(|(index, field, plan)| {
            let (runner, fact_tables) = (&runner, &fact_tables);
            async move {
                let mut node_type = None;
                let outcome = match plan {
                    PlannedRoot::Regular(plan) => runner
                        .execute_planned_regular(*plan, variables, security_context, false)
                        .await
                        .and_then(crate::runtime::ResponseBody::into_value),
                    PlannedRoot::FactTable(plan) => {
                        fact_tables.execute_planned_fact_root(*plan, variables).await
                    },
                    PlannedRoot::Node(plan) => runner
                        .execute_planned_node(*plan, security_context)
                        .await
                        .map(|(value, type_name)| {
                            node_type = type_name;
                            crate::runtime::ResultProjector::wrap_in_data_envelope(
                                value,
                                field.response_key(),
                            )
                        }),
                };
                (index, field, outcome, node_type)
            }
        });
        let outcomes = futures::future::join_all(reads).await;

        // 3. Each root's value under its response key, its runner's errors kept, a failed root
        //    `null` with its error.
        let mut values: std::collections::HashMap<usize, serde_json::Value> =
            std::collections::HashMap::with_capacity(outcomes.len());
        let mut errors = Vec::new();
        let mut data_nulled = false;
        let mut node_types = crate::runtime::completion::ResolvedTypes::default();
        for (index, field, outcome, node_type) in outcomes {
            let key = field.response_key();
            if let Some(type_name) = node_type {
                node_types.insert(key, &type_name);
            }
            match outcome {
                Ok(mut response) => {
                    if let Some(serde_json::Value::Array(reported)) =
                        response.get_mut("errors").map(serde_json::Value::take)
                    {
                        errors.extend(reported);
                    }
                    values.insert(index, response["data"][key].take());
                },
                Err(error) => {
                    errors.push(self.root_error_entry(&error, key));
                    data_nulled |= self.ctx.output_types.is_non_null(root_type, &field.name);
                    values.insert(index, serde_json::Value::Null);
                },
            }
        }

        // Reassemble in request order. A root that `@skip` removed is absent from
        // `resolved` and therefore has no key in `data`, which is what the spec asks for.
        let fields = resolved
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let field_name = f.response_key().to_string();
                let data = if f.name == "__typename" {
                    serde_json::Value::String(root_type.to_string())
                } else {
                    values.remove(&index).unwrap_or(serde_json::Value::Null)
                };
                RootFieldResult { field_name, data }
            })
            .collect();

        Ok(PipelineResult {
            fields,
            parallel: true,
            errors,
            data_nulled,
            resolved: node_types,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
