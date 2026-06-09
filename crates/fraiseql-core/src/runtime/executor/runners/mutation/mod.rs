//! Mutation execution runner.
//!
//! [`MutationRunner`] executes GraphQL mutations with compile-time capability enforcement.
//!
//! The [`execute_mutation_impl`] free function contains the core logic and is bounded only on
//! `A: DatabaseAdapter`, allowing it to be called from both the compile-time-checked
//! [`MutationRunner`] path and the runtime-guarded `execute_mutation_query` path on
//! [`Executor`](super::super::core::Executor).

use std::sync::Arc;

use fraiseql_db::{ChangeLogWrite, ViewName};

use super::{
    super::{context::ExecutorContext, resolve_inject_value},
    query_projection::selections_contain_field,
};
use crate::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    error::{FraiseQLError, Result},
    graphql::{DirectiveEvaluator, FieldSelection},
    runtime::{
        ResultProjector,
        mutation_result::{MutationOutcome, parse_mutation_row},
        project_entity, suggest_similar,
    },
    schema::{CompiledSchema, MutationOperation, NamingConvention},
    security::SecurityContext,
};

/// Enforce the dynamic field authorizer (#423) on a projected mutation payload.
///
/// `entity` is the full projected-from value (the `parent`); `projected` is the
/// response object, mutated in place. Fail-closed:
/// - a gated field selected with no authenticated principal → 403,
/// - a gated field selected with no authorizer configured → 403,
/// - a gated field nested in a sub-selection → 403 (top-level enforced in v1),
/// - a `Reject` decision or any policy error → 403.
///
/// No-op (and zero authorizer calls) when the selection set has no gated field.
fn enforce_mutation_field_authz<A: DatabaseAdapter>(
    ctx: &ExecutorContext<A>,
    security_ctx: Option<&SecurityContext>,
    type_name: &str,
    selections: &[FieldSelection],
    entity: &serde_json::Value,
    projected: &mut serde_json::Value,
) -> Result<()> {
    use crate::security::field_authorizer as authz;

    if !authz::selection_set_selects_gated_field(&ctx.schema, type_name, selections) {
        return Ok(());
    }
    let Some(principal) = security_ctx else {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization is required for a selected field on type \
                 '{type_name}' but the request is not authenticated"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        });
    };
    let Some(authorizer) = ctx.config.field_authorizer.as_ref() else {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization is required for a selected field on type \
                 '{type_name}' but no field authorizer is configured"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        });
    };
    if authz::selection_set_has_nested_gated_field(&ctx.schema, type_name, selections) {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization of nested fields on type '{type_name}' is not \
                 supported in this version"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        });
    }
    let gated = authz::collect_top_level_gated_fields(&ctx.schema, type_name, selections);
    let pass = authz::FieldAuthzPass {
        authorizer: authorizer.as_ref(),
        principal,
        type_name,
        gated: &gated,
        // The mutation path has no static requires_scope gate, so nothing is pre-masked.
        statically_masked: &[],
    };
    authz::apply_field_authorizer_to_entity(&pass, entity, projected)
}

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
        selections: &[FieldSelection],
    ) -> Result<serde_json::Value> {
        execute_mutation_impl(&self.ctx, mutation_name, variables, None, selections).await
    }
}

/// Re-case a mutation input payload's keys from the GraphQL surface naming
/// convention to the schema's canonical (stored) field names, recursing into
/// nested input objects and arrays of input objects.
///
/// The compiled schema stores field names in their canonical (typically
/// `snake_case`) form; with [`NamingConvention::CamelCase`] the GraphQL surface
/// presents them as `camelCase`, so a client sends `camelCase` keys. The Insert
/// path maps those to positional SQL args by name (casing handled implicitly),
/// but the Update path forwards the whole object as one JSONB arg — so without
/// this the SQL function receives surface-cased keys it cannot read (#400).
///
/// Driven by the input type's per-field map (not a lossy `camel→snake` regex),
/// so intentional names / acronyms in the canonical field names are preserved.
/// A [`NamingConvention::Preserve`] schema, an unknown input type, and keys that
/// match no field are all left untouched.
fn recase_input_payload(
    value: serde_json::Value,
    input_type_name: &str,
    schema: &CompiledSchema,
) -> serde_json::Value {
    // Preserve convention: the GraphQL surface already uses the canonical names.
    if schema.naming_convention != NamingConvention::CamelCase {
        return value;
    }
    let Some(input_type) = schema.find_input_type(input_type_name) else {
        return value;
    };
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, val) in map {
                // Match the incoming surface key against each field's surface name,
                // then map back to the exact canonical name.
                let field = input_type.fields.iter().find(|f| schema.display_name(&f.name) == key);
                let canonical = field.map_or(key, |f| f.name.clone());
                let recased = match field
                    .and_then(|f| nested_input_type_name(&f.field_type, schema))
                {
                    Some(nested) => match val {
                        serde_json::Value::Object(_) => recase_input_payload(val, &nested, schema),
                        serde_json::Value::Array(items) => serde_json::Value::Array(
                            items
                                .into_iter()
                                .map(|it| recase_input_payload(it, &nested, schema))
                                .collect(),
                        ),
                        other => other,
                    },
                    None => val,
                };
                out.insert(canonical, recased);
            }
            serde_json::Value::Object(out)
        },
        other => other,
    }
}

/// If `field_type` (e.g. `"BillingAddressInput"`, `"[TagInput!]"`) names a known
/// input object type, return that type's bare name (`[`, `]`, `!` stripped) so
/// [`recase_input_payload`] can recurse; otherwise `None` (scalars, enums, and
/// unknown types are left untouched).
fn nested_input_type_name(field_type: &str, schema: &CompiledSchema) -> Option<String> {
    let base = field_type.trim_matches(|c| c == '[' || c == ']' || c == '!');
    schema.find_input_type(base).map(|_| base.to_string())
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
    selections: &[FieldSelection],
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

    // 1a. Operation-level authorization (#422): the universal mutation chokepoint.
    //     EVERY mutation entry path converges here — the two `*_internal` GraphQL
    //     branches, `execute_mutation_query`, and the direct `SupportsMutations` API
    //     used by the anonymous-REST write path (which bypasses both chokepoints).
    //     Runs after `find_mutation` so an unknown name keeps its "not found" message
    //     (no enumeration leak), and before `requires_role` (AND-composition).
    //     Fail-closed: a `Deny` or any policy error returns 403.
    if let Some(authorizer) = ctx.config.authorizer.as_ref() {
        let ops =
            [(crate::security::authorizer::OperationKind::Mutation, mutation_name.to_string())];
        crate::security::authorizer::enforce_authz(
            authorizer.as_ref(),
            security_ctx,
            &ops,
            variables,
        )?;
    }

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

    if let Some(input_type_name) = input_type_name.filter(|_| is_update) {
        // Pass the entire input object as a single JSONB arg, re-cased from the
        // GraphQL surface naming convention to the schema's canonical (stored)
        // field names so the SQL function — which reads `payload->>'snake_field'`
        // / jsonb_populate_record — sees the values. The Insert path below gets
        // this for free via positional args; the Update path must do it
        // explicitly (fixes #400).
        let input_obj = vars_obj.and_then(|obj| obj.get("input")).and_then(|v| v.as_object());
        if let Some(obj) = input_obj {
            let payload = serde_json::Value::Object(obj.clone());
            args.push(recase_input_payload(payload, input_type_name, &ctx.schema));
        } else if !mutation_def.arguments[0].nullable {
            missing_required.push("input");
        }
    } else if let Some(input_type) = input_type_name.and_then(|n| ctx.schema.find_input_type(n)) {
        // Insert / Delete / Custom: flatten Input type fields to positional typed args.
        let input_obj = vars_obj.and_then(|obj| obj.get("input")).and_then(|v| v.as_object());
        if let Some(input_obj) = input_obj {
            // #414: enforce required (non-null, no-default) input fields before the
            // database call, rejecting an omitted-or-explicit-null required field
            // with a GraphQL validation error instead of passing SQL NULL through.
            //
            // Look up each field by its GraphQL surface name (`display_name`):
            // under `NamingConvention::CamelCase` the client sends camelCase keys
            // while `input_type.fields` hold canonical (snake_case) names. Using
            // the surface key both makes the required check correct and fixes the
            // latent value-passing miss on the camelCase Insert path (previously
            // only the Update path recased — see `recase_input_payload`).
            let mut missing_input_fields: Vec<&str> = Vec::new();
            for field in &input_type.fields {
                let key = ctx.schema.display_name(&field.name);
                let value = input_obj.get(&key);
                if field.is_required() && value.is_none_or(serde_json::Value::is_null) {
                    missing_input_fields.push(field.name.as_str());
                }
                args.push(value.cloned().unwrap_or(serde_json::Value::Null));
            }
            if !missing_input_fields.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Mutation '{mutation_name}': required input field(s) not provided or \
                         null: {}",
                        missing_input_fields.join(", ")
                    ),
                    path:    None,
                });
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

    // 3b. Resolve session variables once and pass them to the adapter call so
    //     they are applied on the same connection / transaction as the function
    //     (fixes #329 — set_config(..., true) is transaction-local, so applying
    //     it on a separate pooled connection left it invisible to the function).
    //
    // Only resolved when there are variables to inject or inject_started_at is
    // enabled, and only on the authenticated path (security context present).
    // The no-op default on non-PostgreSQL adapters means an empty slice here is
    // effectively free there.
    let resolved_session_vars = {
        let sv = &ctx.schema.session_variables;
        match security_ctx {
            Some(sec_ctx) if !sv.variables.is_empty() || sv.inject_started_at => {
                crate::runtime::executor::security::resolve_session_variables(sv, sec_ctx)
            },
            _ => Vec::new(),
        }
    };
    let session_pairs: Vec<(&str, &str)> =
        resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    // 4. Call the database function (session variables pinned to its connection) AND write the
    //    change-log outbox row in the same transaction — the Change Spine transactional outbox. The
    //    framework owns this write by default; apps drop their hand-rolled per-mutation-function
    //    inserts on upgrade (a documented breaking change). The adapter reads the changed-entity
    //    columns (object_id / object_data / updated_fields / cascade) from the function's own
    //    mutation_response row; only the DML verb and a NOT-NULL object_type fallback (the GraphQL
    //    return type) are threaded down here. Non-PostgreSQL adapters ignore the change-log
    //    descriptor (multi-DB parity lands in phase-03).
    //
    //    Opt-out (default-on): a row is written only when the global switch
    //    (`RuntimeConfig.changelog_enabled`) is on AND this mutation is not
    //    individually opted out (`MutationDefinition.changelog`). Passing `None`
    //    makes the adapter behave exactly like the session-affine path.
    let modification_type = mutation_def.operation.kind_str().to_uppercase();
    let write_changelog = ctx.config.changelog_enabled && mutation_def.changelog;
    // Envelope stamp (phase-03): stamp the tenant partition id EXPLICITLY from the
    // SecurityContext — never reconstructed from connection / RLS state, because
    // out-of-session spine consumers (poller, NATS bridge) bypass RLS and must
    // re-authz fan-out from the row itself. `tenant_id` is the Trinity
    // public-facing UUID; a request with no tenant, or a tenant identifier that
    // is not UUID-shaped, leaves it NULL (we never abort a user's mutation over a
    // log-row stamp). `actor_type`/`schema_version`/trace context stay NULL here
    // pending #390 / #377 / #375.
    let tenant_uuid = security_ctx
        .and_then(|c| c.tenant_id.as_ref())
        .and_then(|t| uuid::Uuid::parse_str(t.as_str()).ok());
    let changelog = write_changelog.then(|| {
        ChangeLogWrite::new(&mutation_def.return_type, &modification_type)
            .with_tenant_id(tenant_uuid)
    });
    let rows = ctx
        .adapter
        .execute_function_call_with_changelog(sql_source, &args, &session_pairs, changelog.as_ref())
        .await?;

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

    // Evaluate @skip / @include against the request variables before projecting, so
    // conditional fields are honoured exactly as on the query path. (Named fragment
    // spreads were already resolved at classification time, where the document's
    // fragment definitions are available.)
    let variables_map: std::collections::HashMap<String, serde_json::Value> = match variables {
        Some(serde_json::Value::Object(map)) => {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        },
        _ => std::collections::HashMap::new(),
    };
    let filtered_selections = DirectiveEvaluator::filter_selections(selections, &variables_map)
        .map_err(|e| FraiseQLError::Validation {
            message: e.to_string(),
            path:    Some("directives".to_string()),
        })?;
    let selections: &[FieldSelection] = &filtered_selections;

    let result_json = match outcome {
        MutationOutcome::Success {
            entity,
            entity_type,
            cascade,
            ..
        } => {
            // Resolve the concrete GraphQL type of the success entity: the
            // mutation_response's entity_type, else the first non-error union
            // member, else the declared return type.
            let typename = entity_type
                .or_else(|| {
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

            // Project the entity through the single canonical projector — the same
            // snake_case source keys, surface output keys, depth-aware recursion and
            // selection-gated __typename as the query path — so a mutation's success
            // payload and a query over the same entity return an identical shape.
            let mut projected = project_entity(&entity, &typename, selections, &ctx.schema);

            // Enforce the dynamic field authorizer (#423) on the success entity, per
            // the resolved concrete type, before surfacing it. Fail-closed.
            enforce_mutation_field_authz(
                ctx,
                security_ctx,
                &typename,
                selections,
                &entity,
                &mut projected,
            )?;

            // Surface the graphql-cascade wire format
            // (updated/deleted/invalidations/metadata) to clients without requiring
            // the DB function to embed it in the entity JSONB itself.
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

            // Find the matching error type from the return union.
            let error_type = ctx.schema.find_union(&mutation_return_type).and_then(|u| {
                u.member_types.iter().find_map(|t| {
                    let td = ctx.schema.find_type(t)?;
                    if td.is_error { Some(td) } else { None }
                })
            });

            // Project error metadata through the same canonical projector when the
            // schema declares a matching error type. Otherwise emit just __typename
            // (only when selected, matching the query contract); status is attached
            // below in both cases.
            let mut result = if let Some(td) = error_type {
                project_entity(&metadata, td.name.as_str(), selections, &ctx.schema)
            } else {
                let mut map = serde_json::Map::new();
                // Scan recursively: `__typename` may be nested inside an inline
                // fragment (`... on T { __typename }`), not just at the top level.
                if selections_contain_field(selections, "__typename") {
                    map.insert(
                        "__typename".to_string(),
                        serde_json::Value::String(mutation_return_type.clone()),
                    );
                }
                serde_json::Value::Object(map)
            };

            // Enforce the dynamic field authorizer (#423) on error metadata too, so a
            // gated field on an error type cannot leak through the error arm.
            if let Some(td) = error_type {
                enforce_mutation_field_authz(
                    ctx,
                    security_ctx,
                    td.name.as_str(),
                    selections,
                    &metadata,
                    &mut result,
                )?;
            }

            // Inject the synthetic `status` field — not part of the type definition,
            // but required by clients to discriminate error outcomes.
            if let serde_json::Value::Object(ref mut map) = result {
                map.insert("status".to_string(), serde_json::Value::String(status.to_string()));
            }

            result
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
