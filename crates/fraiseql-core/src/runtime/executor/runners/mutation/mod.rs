//! Mutation execution runner.
//!
//! [`MutationRunner`] executes GraphQL mutations with compile-time capability enforcement.
//!
//! The [`execute_mutation_impl`] free function contains the core logic and is bounded only on
//! `A: DatabaseAdapter`, allowing it to be called from both the compile-time-checked
//! [`MutationRunner`] path and the runtime-guarded `execute_mutation_query` path on
//! [`Executor`](super::super::core::Executor).

use std::{collections::HashMap, sync::Arc};

use fraiseql_db::ViewName;

use super::super::{context::ExecutorContext, resolve_inject_value};
use crate::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    error::{FraiseQLError, Result},
    runtime::{
        FieldMapping, ProjectionMapper, ResultProjector, build_field_mappings_from_type,
        mutation_result::{MutationOutcome, parse_mutation_row},
        suggest_similar,
    },
    schema::MutationOperation,
    security::SecurityContext,
};

/// Executes GraphQL mutations with compile-time capability enforcement.
///
/// Only constructible when `A: SupportsMutations`. This means calling mutation
/// methods on an executor backed by `SqliteAdapter` (which does not implement
/// `SupportsMutations`) is a compiler error, not a runtime failure.
pub(in super::super) struct MutationRunner<A: DatabaseAdapter + SupportsMutations> {
    ctx: Arc<ExecutorContext<A>>,
}

impl<A: DatabaseAdapter + SupportsMutations> MutationRunner<A> {
    /// Create a new `MutationRunner` from a shared executor context.
    ///
    /// Zero-cost: `Arc` is already shared — this is just a newtype wrapper.
    pub(in super::super) const fn new(ctx: Arc<ExecutorContext<A>>) -> Self {
        Self { ctx }
    }

    /// Execute a GraphQL mutation with compile-time [`SupportsMutations`] enforcement.
    ///
    /// # Errors
    ///
    /// Same as [`execute_mutation_impl`].
    pub(in super::super) async fn execute_mutation(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        type_selections: &HashMap<String, Vec<String>>,
    ) -> Result<serde_json::Value> {
        execute_mutation_impl(&self.ctx, mutation_name, variables, None, type_selections).await
    }
}

/// Core mutation execution logic, bounded only on `A: DatabaseAdapter`.
///
/// Called from:
/// - [`MutationRunner::execute_mutation`] — compile-time [`SupportsMutations`] path
/// - `Executor::execute_mutation_query` — runtime-guarded path (raw GraphQL dispatch)
/// - `execute_with_security_internal` — authenticated GraphQL dispatch
///
/// The caller is responsible for ensuring the adapter supports mutations before calling
/// this function (either via the compile-time `SupportsMutations` bound or a runtime
/// `supports_mutations()` guard).
///
/// # Errors
///
/// * [`FraiseQLError::Validation`] — mutation not found, no `sql_source`, missing security context
///   for `inject` params, or database function returned no rows.
/// * [`FraiseQLError::Database`] — the adapter's `execute_function_call` failed.
pub(in super::super) async fn execute_mutation_impl<A: DatabaseAdapter>(
    ctx: &ExecutorContext<A>,
    mutation_name: &str,
    variables: Option<&serde_json::Value>,
    security_ctx: Option<&SecurityContext>,
    type_selections: &HashMap<String, Vec<String>>,
) -> Result<serde_json::Value> {
    // 1. Locate the mutation definition
    let mutation_def = ctx.schema.find_mutation(mutation_name).ok_or_else(|| {
        let display_names: Vec<String> =
            ctx.schema.mutations.iter().map(|m| ctx.schema.display_name(&m.name)).collect();
        let candidate_refs: Vec<&str> = display_names.iter().map(String::as_str).collect();
        let suggestion = suggest_similar(mutation_name, &candidate_refs);
        let message = match suggestion.as_slice() {
            [s] => {
                format!("Mutation '{mutation_name}' not found in schema. Did you mean '{s}'?")
            },
            [a, b] => format!(
                "Mutation '{mutation_name}' not found in schema. Did you mean '{a}' or '{b}'?"
            ),
            [a, b, c, ..] => format!(
                "Mutation '{mutation_name}' not found in schema. Did you mean '{a}', '{b}', or \
                 '{c}'?"
            ),
            _ => format!("Mutation '{mutation_name}' not found in schema"),
        };
        FraiseQLError::Validation {
            message,
            path: None,
        }
    })?;

    // 1b. Enforce requires_role — return "not found" (not "forbidden") to prevent
    //     enumeration, mirroring the query-level check in query_regular.rs.
    if let Some(required_role) = mutation_def.requires_role.as_deref() {
        let has_role = security_ctx.is_some_and(|c| c.roles.iter().any(|r| r == required_role));
        if !has_role {
            return Err(FraiseQLError::Validation {
                message: format!("Mutation '{mutation_name}' not found in schema"),
                path:    None,
            });
        }
    }

    // 2. Require a sql_source (PostgreSQL function name).
    //
    // Fall back to the operation's table field when sql_source is absent.
    // The CLI compiler stores the SQL function name in both places
    // (sql_source and operation.{Insert|Update|Delete}.table), but older or
    // alternate compilation paths (e.g. fraiseql-core's own codegen) may only
    // populate operation.table and leave sql_source as None.
    let sql_source_owned: String;
    let sql_source: &str = if let Some(src) = mutation_def.sql_source.as_deref() {
        src
    } else {
        match &mutation_def.operation {
            MutationOperation::Insert { table }
            | MutationOperation::Update { table }
            | MutationOperation::Delete { table }
                if !table.is_empty() =>
            {
                sql_source_owned = table.clone();
                &sql_source_owned
            },
            _ => {
                return Err(FraiseQLError::Validation {
                    message: format!("Mutation '{mutation_name}' has no sql_source configured"),
                    path:    None,
                });
            },
        }
    };

    // 3. Build positional args Vec from variables in ArgumentDefinition order. Validate that every
    //    required (non-nullable, no default) argument is present.
    //
    //    Input object unwrapping: when the mutation has a single argument named "input"
    //    whose type is an Input type, AND the client sends a JSON object for that argument,
    //    unwrap the object's fields and pass them positionally in the order defined by the
    //    input type's field list.  This keeps the SQL function signature flat while letting
    //    the GraphQL API use the standard input object pattern.
    let vars_obj = variables.and_then(|v| v.as_object());

    let mut missing_required: Vec<&str> = Vec::new();
    let total_args = mutation_def.arguments.len() + mutation_def.inject_params.len();
    let mut args: Vec<serde_json::Value> = Vec::with_capacity(total_args);

    // Detect single-input-object pattern
    let input_type_name =
        if mutation_def.arguments.len() == 1 && mutation_def.arguments[0].name == "input" {
            match &mutation_def.arguments[0].arg_type {
                crate::schema::FieldType::Input(name) => Some(name.as_str()),
                _ => None,
            }
        } else {
            None
        };

    // Update mutations pass the entire input object as a single JSONB arg, which
    // preserves all three field states that typed positional args cannot express:
    //   - key absent            → leave the database value unchanged
    //   - key present, null     → SET field = NULL
    //   - key present, value    → SET field = <value>
    // SQL update functions use `input_payload ? 'field'` to test key presence.
    //
    // Insert / Delete / Custom flatten the Input type fields to positional args as
    // before (no three-state problem: absent ≡ NULL for creates; deletes need only
    // the PK).
    let is_update = matches!(&mutation_def.operation, MutationOperation::Update { .. });

    if is_update && input_type_name.is_some() {
        // Pass the entire input object as a single JSONB arg.
        let input_obj = vars_obj.and_then(|obj| obj.get("input")).and_then(|v| v.as_object());
        if let Some(obj) = input_obj {
            args.push(serde_json::Value::Object(obj.clone()));
        } else if !mutation_def.arguments[0].nullable {
            missing_required.push("input");
        }
    } else if let Some(input_type) = input_type_name.and_then(|n| ctx.schema.find_input_type(n)) {
        // Insert / Delete / Custom: flatten Input type fields to positional typed args.
        let input_obj = vars_obj.and_then(|obj| obj.get("input")).and_then(|v| v.as_object());
        if let Some(input_obj) = input_obj {
            for field in &input_type.fields {
                let value = input_obj.get(&field.name).cloned();
                args.push(value.unwrap_or(serde_json::Value::Null));
            }
        } else if !mutation_def.arguments[0].nullable {
            missing_required.push("input");
        }
    } else {
        // Standard argument handling (flat arguments, no input object)
        args.extend(mutation_def.arguments.iter().map(|arg| {
            let value = vars_obj.and_then(|obj| obj.get(&arg.name)).cloned();
            if let Some(v) = value {
                v
            } else {
                if !arg.nullable && arg.default_value.is_none() {
                    missing_required.push(&arg.name);
                }
                arg.default_value.as_ref().map_or(serde_json::Value::Null, |v| v.to_json())
            }
        }));
    }

    if !missing_required.is_empty() {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Mutation '{mutation_name}' is missing required argument(s): {}",
                missing_required.join(", ")
            ),
            path:    None,
        });
    }

    // 3a. Append server-injected parameters (after client args, in injection order).
    //
    // CONTRACT: inject params are always the *last* positional parameters of the SQL
    // function, in the order they appear in `inject_params` (insertion-ordered IndexMap).
    // The SQL function signature in the database MUST declare injected parameters after
    // all client-supplied parameters. Violating this order silently passes inject values
    // to the wrong SQL parameters. The CLI compiler (`fraiseql-cli compile`) validates
    // inject key names and source syntax when producing `schema.compiled.json`, but
    // cannot verify SQL function arity — that remains a developer responsibility.
    if !mutation_def.inject_params.is_empty() {
        let sec_ctx = security_ctx.ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Mutation '{}' requires inject params but no security context is available \
                 (unauthenticated request)",
                mutation_name
            ),
            path:    None,
        })?;
        for (param_name, source) in &mutation_def.inject_params {
            args.push(resolve_inject_value(param_name, source, sec_ctx)?);
        }
    }

    // 3b. Inject session variables (transaction-scoped set_config) when configured.
    //
    // Only called when there are variables to inject or inject_started_at is enabled,
    // and only on the authenticated path (security context present). The no-op default
    // on non-PostgreSQL adapters means this call is effectively free there.
    {
        let sv = &ctx.schema.session_variables;
        if !sv.variables.is_empty() || sv.inject_started_at {
            if let Some(sec_ctx) = security_ctx {
                let vars =
                    crate::runtime::executor::security::resolve_session_variables(sv, sec_ctx);
                let pairs: Vec<(&str, &str)> =
                    vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                ctx.adapter.set_session_variables(&pairs).await?;
            }
        }
    }

    // 4. Call the database function
    let rows = ctx.adapter.execute_function_call(sql_source, &args).await?;

    // 5. Expect at least one row
    let row = rows.into_iter().next().ok_or_else(|| FraiseQLError::Validation {
        message: format!("Mutation '{mutation_name}': function returned no rows"),
        path:    None,
    })?;

    // 6. Parse the mutation_response row
    let outcome = parse_mutation_row(&row)?;

    // 6a. Bump fact table versions after a successful mutation.
    //
    // This invalidates cached aggregation results for any fact tables listed
    // in `MutationDefinition.invalidates_fact_tables`.  We bump versions on
    // Success only — an Error outcome means no data was written, so caches
    // remain valid.  Non-cached adapters return Ok(()) from the default trait
    // implementation (no-op); only `CachedDatabaseAdapter` performs actual work.
    if matches!(outcome, MutationOutcome::Success { .. })
        && !mutation_def.invalidates_fact_tables.is_empty()
    {
        ctx.adapter
            .bump_fact_table_versions(&mutation_def.invalidates_fact_tables)
            .await?;
    }

    // Invalidate query result cache for views/entities touched by this mutation.
    //
    // Strategy:
    // - UPDATE/DELETE with entity_id: entity-aware eviction only (precise, no false positives).
    //   Evicts only the cache entries that actually contain the mutated entity UUID.
    // - CREATE or explicit invalidates_views: view-level flush. For CREATE the new entity isn't in
    //   any existing cache entry, so entity-aware is a no-op. View-level ensures list queries
    //   return the new row.
    // - No entity_id and no views declared: infer view from return type (backward-compat).
    if let MutationOutcome::Success {
        entity_type,
        entity_id,
        ..
    } = &outcome
    {
        // Entity-aware path: precise eviction for UPDATE/DELETE.
        if let (Some(etype), Some(eid)) = (entity_type.as_deref(), entity_id.as_deref()) {
            ctx.adapter.invalidate_by_entity(etype, eid).await?;

            // The response cache doesn't have entity-level granularity, so
            // invalidate by the inferred view for this entity type.
            if let Some(ref rc) = ctx.response_cache {
                let inferred_view = ctx
                    .schema
                    .types
                    .iter()
                    .find(|t| t.name == etype)
                    .filter(|t| !t.sql_source.as_str().is_empty())
                    .map(|t| t.sql_source.to_string());
                if let Some(view) = inferred_view {
                    let _ = rc.invalidate_views(&[ViewName::from(view)]);
                }
            }
        }

        // View-level path: needed when entity_id is absent (CREATE) or when the developer
        // explicitly declared invalidates_views to also refresh list queries.
        if entity_id.is_none() || !mutation_def.invalidates_views.is_empty() {
            // Promote the schema's `Vec<String>` view list into `Vec<ViewName>`
            // once — every downstream invalidator borrows the same Arc<str>.
            let views_to_invalidate: Vec<ViewName> = if mutation_def.invalidates_views.is_empty() {
                ctx.schema
                    .types
                    .iter()
                    .find(|t| t.name == mutation_def.return_type)
                    .filter(|t| !t.sql_source.as_str().is_empty())
                    .map(|t| ViewName::from(t.sql_source.as_str()))
                    .into_iter()
                    .collect()
            } else {
                mutation_def.invalidates_views.iter().map(ViewName::from).collect()
            };
            if !views_to_invalidate.is_empty() {
                if entity_id.is_none() {
                    // CREATE: the new entity is absent from all existing cache entries,
                    // so point-lookup entries for other entities remain valid.  Only
                    // list queries need eviction (the new row must appear in results).
                    ctx.adapter.invalidate_list_queries(&views_to_invalidate).await?;
                } else {
                    // Developer-declared invalidates_views on an UPDATE/DELETE: honour
                    // the explicit annotation with a full view sweep.
                    ctx.adapter.invalidate_views(&views_to_invalidate).await?;
                }
                // Also invalidate the response cache for these views
                if let Some(ref rc) = ctx.response_cache {
                    let _ = rc.invalidate_views(&views_to_invalidate);
                }
            }
        }
    }

    // Clone name and return_type to avoid borrow issues after schema lookups
    let mutation_return_type = mutation_def.return_type.clone();
    let mutation_name_owned = mutation_name.to_string();

    // Helper: merge common fields (key "") with type-specific fields for selection filtering.
    let selection_for_type = |type_name: &str| -> Option<Vec<String>> {
        if type_selections.is_empty() {
            return None;
        }
        let common = type_selections.get("");
        let specific = type_selections.get(type_name);
        match (common, specific) {
            (None, None) => None,
            (Some(c), None) => Some(c.clone()),
            (None, Some(s)) => Some(s.clone()),
            (Some(c), Some(s)) => {
                let mut merged = c.clone();
                merged.extend(s.iter().cloned());
                Some(merged)
            },
        }
    };

    let result_json = match outcome {
        MutationOutcome::Success {
            entity,
            entity_type,
            cascade,
            ..
        } => {
            // Determine the GraphQL __typename
            let typename = entity_type
                .or_else(|| {
                    // Fall back to first non-error union member
                    ctx.schema
                        .find_union(&mutation_return_type)
                        .and_then(|u| {
                            u.member_types
                                .iter()
                                .find(|t| ctx.schema.find_type(t).is_none_or(|td| !td.is_error))
                        })
                        .cloned()
                })
                .unwrap_or_else(|| mutation_return_type.clone());

            // Build projection mappings from the selection set.
            // Success entities use snake_case keys (from DB), so source == output.
            let requested = selection_for_type(&typename);
            let mappings: Vec<FieldMapping> = match &requested {
                Some(fields) => fields.iter().map(|f| FieldMapping::simple(f.clone())).collect(),
                None => {
                    // No selection filtering — pass all fields
                    entity
                        .as_object()
                        .map(|m| m.keys().map(|k| FieldMapping::simple(k.clone())).collect())
                        .unwrap_or_default()
                },
            };

            let mapper = ProjectionMapper::with_mappings(mappings).with_typename(&typename);
            let obj = entity.as_object().cloned().unwrap_or_default();
            let mut projected = mapper.project_json_object(&obj)?;

            // Inject cascade JSONB into the projected object when present.
            // This surfaces the graphql-cascade wire format
            // (updated/deleted/invalidations/metadata) to clients without
            // requiring the DB function to embed it in the entity JSONB itself.
            if let Some(cascade_json) = cascade {
                if let serde_json::Value::Object(ref mut map) = projected {
                    map.insert("cascade".to_string(), cascade_json);
                }
            }

            projected
        },
        MutationOutcome::Error {
            error_class,
            metadata,
            ..
        } => {
            let status = error_class.as_str();

            // Find the matching error type from the return union
            let error_type = ctx.schema.find_union(&mutation_return_type).and_then(|u| {
                u.member_types.iter().find_map(|t| {
                    let td = ctx.schema.find_type(t)?;
                    if td.is_error { Some(td) } else { None }
                })
            });

            match error_type {
                Some(td) => {
                    // Build field mappings from the error type definition, with camelCase
                    // source keys and recursive nested object/array projection (#215).
                    let requested = selection_for_type(td.name.as_str());
                    let requested_slice = requested.as_deref();
                    let mut visited = std::collections::HashSet::new();
                    let mappings = build_field_mappings_from_type(
                        &td.fields,
                        &ctx.schema,
                        requested_slice,
                        &mut visited,
                    );

                    let mapper = ProjectionMapper::with_mappings(mappings)
                        .with_typename(td.name.to_string());
                    let obj = metadata.as_object().cloned().unwrap_or_default();
                    let mut result = mapper.project_json_object(&obj)?;

                    // Inject status (not in type definition, but required by clients)
                    if let serde_json::Value::Object(ref mut map) = result {
                        map.insert(
                            "status".to_string(),
                            serde_json::Value::String(status.to_string()),
                        );
                    }

                    result
                },
                None => {
                    // No error type defined: surface the status as a plain object
                    serde_json::json!({ "__typename": mutation_return_type, "status": status })
                },
            }
        },
    };

    // 7. Emit structured mutation audit event when audit_mutations is enabled.
    //
    // This is the single chokepoint for all mutation paths (GraphQL handler,
    // REST handler, typed execute_mutation, bulk filter). Zero-cost when disabled:
    // the branch is not taken and no string formatting or allocation occurs.
    if ctx.config.audit_mutations {
        tracing::info!(
            target: "fraiseql::mutation_audit",
            mutation_name = mutation_name,
            entity_type = %mutation_def.return_type,
            operation = %mutation_def.operation.kind_str(),
            tenant_id = %security_ctx
                .and_then(|c| c.tenant_id.as_ref().map(|t| t.as_str()))
                .unwrap_or(""),
            "mutation.executed"
        );
    }

    let response = ResultProjector::wrap_in_data_envelope(result_json, &mutation_name_owned);
    Ok(response)
}

#[cfg(test)]
mod tests;
