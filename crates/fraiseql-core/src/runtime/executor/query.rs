//! Regular and relay query execution.

use std::sync::Arc;

use super::{Executor, null_masked_fields, resolve_inject_value};
use crate::{
    db::{
        CursorValue, ProjectionField, WhereClause, WhereOperator,
        projection_generator::{FieldKind, PostgresProjectionGenerator},
        traits::DatabaseAdapter,
    },
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::{JsonbStrategy, ResultProjector},
    schema::{CompiledSchema, SqlProjectionHint},
    security::{RlsWhereClause, SecurityContext},
};

/// Build a recursive [`ProjectionField`] tree from a GraphQL selection set.
///
/// For each field in `selections`, consults the compiled schema to determine
/// whether the field is composite (Object) or scalar, and — for Object fields —
/// recurses into the requested sub-fields to produce a nested
/// `jsonb_build_object(...)` at the SQL level instead of returning the full blob.
///
/// List fields always fall back to `data->'field'` (full blob) because
/// sub-projection inside aggregated JSONB arrays is out of scope.
///
/// Recursion is capped at 4 levels, matching `MAX_PROJECTION_DEPTH` in the
/// projection generator.
///
/// Filter `__typename` from SQL projection fields.
/// `__typename` is a GraphQL meta-field not stored in JSONB.
/// The `ResultProjector` handles injection — see `projection.rs`.
/// Removing this filter causes `data->>'__typename'` (NULL) to overwrite
/// the value injected by `with_typename()`, depending on field iteration order.
fn build_typed_projection_fields(
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    parent_type_name: &str,
    depth: usize,
) -> Vec<ProjectionField> {
    const MAX_DEPTH: usize = 4;

    let type_def = schema.find_type(parent_type_name);
    selections
        .iter()
        // Skip __typename — it is a GraphQL meta-field not stored in the JSONB column.
        // Including it would generate `data->>'__typename'` (always NULL) in the SQL
        // projection and then overwrite the value already injected by `with_typename`.
        .filter(|sel| sel.name != "__typename")
        .map(|sel| {
            let field_def =
                type_def.and_then(|td| td.fields.iter().find(|f| f.name == sel.name.as_str()));

            let is_composite = field_def.is_some_and(|fd| !fd.field_type.is_scalar());
            let is_list = field_def.is_some_and(|fd| fd.field_type.is_list());
            let is_text = field_def.is_some_and(|fd| {
                matches!(
                    fd.field_type,
                    crate::schema::FieldType::String | crate::schema::FieldType::Id
                )
            });

            let kind = if is_composite {
                FieldKind::Composite
            } else if is_text {
                FieldKind::Text
            } else {
                FieldKind::Native
            };

            // Recurse into Object types only — List fields fall back to full blob
            let sub_fields =
                if is_composite && !is_list && !sel.nested_fields.is_empty() && depth < MAX_DEPTH {
                    let child_type =
                        field_def.and_then(|fd| fd.field_type.type_name()).unwrap_or("");
                    if child_type.is_empty() {
                        None
                    } else {
                        Some(build_typed_projection_fields(
                            &sel.nested_fields,
                            schema,
                            child_type,
                            depth + 1,
                        ))
                    }
                } else {
                    None
                };

            ProjectionField {
                name: sel.response_key().to_string(),
                kind,
                sub_fields,
            }
        })
        .collect()
}

/// Map a schema [`FieldType`] to the ORDER BY cast hint.
///
/// Returns [`OrderByFieldType::Text`] for types that sort correctly as text
/// (strings, UUIDs, enums) or for composite/container types where a cast
/// would be meaningless.
const fn field_type_to_order_by_type(ft: &crate::schema::FieldType) -> crate::db::OrderByFieldType {
    use crate::{db::OrderByFieldType as OB, schema::FieldType as FT};
    match ft {
        FT::Int => OB::Integer,
        FT::Float | FT::Decimal => OB::Numeric,
        FT::Boolean => OB::Boolean,
        FT::DateTime => OB::DateTime,
        FT::Date => OB::Date,
        FT::Time => OB::Time,
        // String, ID, UUID, Json, Enum, Scalar, and container types sort as text.
        _ => OB::Text,
    }
}

/// Enrich parsed `OrderByClause` values with schema-derived type information
/// and native column mappings.
///
/// For each clause, looks up the field in the compiled schema's type definition
/// to determine the correct `OrderByFieldType` (so the SQL generator emits a
/// typed cast), and checks `native_columns` for a direct column mapping (so the
/// SQL generator can bypass JSONB extraction entirely).
fn enrich_order_by_clauses(
    mut clauses: Vec<crate::db::OrderByClause>,
    schema: &CompiledSchema,
    return_type: &str,
    native_columns: &std::collections::HashMap<String, String>,
) -> Vec<crate::db::OrderByClause> {
    let type_def = schema.find_type(return_type);
    for clause in &mut clauses {
        // Look up the field type from the schema definition.
        if let Some(td) = type_def {
            if let Some(field_def) = td.find_field(&clause.field) {
                clause.field_type = field_type_to_order_by_type(&field_def.field_type);
            }
        }

        // Check if the query definition has a native column mapping for this field.
        // `native_columns` keys are the GraphQL argument names (camelCase).
        let storage_key = clause.storage_key();
        if native_columns.contains_key(&storage_key) {
            clause.native_column = Some(storage_key);
        }
    }
    clauses
}

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a regular query with row-level security (RLS) filtering.
    ///
    /// This method:
    /// 1. Validates the user's security context (token expiration, etc.)
    /// 2. Evaluates RLS policies to determine what rows the user can access
    /// 3. Composes RLS filters with user-provided WHERE clauses
    /// 4. Passes the composed filter to the database adapter for SQL-level filtering
    ///
    /// RLS filtering happens at the database level, not in Rust, ensuring:
    /// - High performance (database can optimize filters)
    /// - Correct handling of pagination (LIMIT applied after RLS filtering)
    /// - Type-safe composition via `WhereClause` enum
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — security token expired, role check failed, or query not
    ///   found in schema.
    /// * [`FraiseQLError::Database`] — the database adapter returned an error.
    pub(super) async fn execute_regular_query_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<serde_json::Value> {
        // 1. Validate security context (check expiration, etc.)
        if security_context.is_expired() {
            return Err(FraiseQLError::Validation {
                message: "Security token has expired".to_string(),
                path:    Some("request.authorization".to_string()),
            });
        }

        // 2. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // 2b. Enforce requires_role — return "not found" (not "forbidden") to prevent enumeration
        if let Some(ref required_role) = query_match.query_def.requires_role {
            if !security_context.roles.iter().any(|r| r == required_role) {
                return Err(FraiseQLError::Validation {
                    message: format!("Query '{}' not found in schema", query_match.query_def.name),
                    path:    None,
                });
            }
        }

        // Resolve session variables once for all query methods.
        //
        // Session variables are now passed directly to each database adapter method
        // (execute_where_query, execute_with_projection, etc.) instead of being set
        // globally on the connection. This ensures variables are scoped to the
        // transaction containing both SET and the query.
        let session_vars: Vec<(String, String)> = {
            let sv = &self.schema.session_variables;
            if !sv.variables.is_empty() || sv.inject_started_at {
                let vars = crate::runtime::executor::security::resolve_session_variables(
                    sv,
                    security_context,
                );
                vars.into_iter().collect()
            } else {
                Vec::new()
            }
        };
        let session_var_refs: Vec<(&str, &str)> = session_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        // Route relay queries to dedicated handler with security context.
        if query_match.query_def.relay {
            return self.execute_relay_query(&query_match, variables, Some(security_context)).await;
        }

        // 0. Check response cache (skips all projection/RBAC/serialization work on hit)
        let response_cache_key =
            self.response_cache.as_ref().filter(|rc| rc.is_enabled()).map(|_| {
                let query_key = Self::compute_response_cache_key(&query_match);
                let sec_hash =
                    crate::cache::response_cache::hash_security_context(Some(security_context));
                (query_key, sec_hash)
            });

        if let (Some((query_key, sec_hash)), Some(rc)) =
            (response_cache_key, self.response_cache.as_ref())
        {
            if let Some(cached) = rc.get(query_key, sec_hash)? {
                return Ok((*cached).clone());
            }
        }

        // 3. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 4. Evaluate RLS policy and build WHERE clause filter. The return type is
        //    Option<RlsWhereClause> — a compile-time proof that the clause passed through RLS
        //    evaluation.
        let rls_where_clause: Option<RlsWhereClause> =
            if let Some(ref rls_policy) = self.config.rls_policy {
                // Evaluate RLS policy with user's security context
                rls_policy.evaluate(security_context, &query_match.query_def.name)?
            } else {
                // No RLS policy configured, allow all access
                None
            };

        // 5. Get SQL source from query definition
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // 6. Generate SQL projection hint for requested fields (optimization). Build a recursive
        //    ProjectionField tree from the selection set so that composite sub-fields are projected
        //    with nested jsonb_build_object instead of returning the full blob.
        let projection_hint = if !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let root_fields = query_match
                .selections
                .first()
                .map_or(&[] as &[_], |s| s.nested_fields.as_slice());
            let typed_fields = build_typed_projection_fields(
                root_fields,
                &self.schema,
                &query_match.query_def.return_type,
                0,
            );

            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_typed_projection_sql(&typed_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint {
                database:                    self.adapter.database_type(),
                projection_template:         projection_sql,
                estimated_reduction_percent: compute_projection_reduction(
                    plan.projection_fields.len(),
                ),
            })
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        // 7. AND inject conditions onto the RLS WHERE clause. Inject conditions always come after
        //    RLS so they cannot bypass it.
        let combined_where: Option<WhereClause> = if query_match.query_def.inject_params.is_empty()
        {
            // Common path: unwrap RlsWhereClause into WhereClause for the adapter
            rls_where_clause.map(RlsWhereClause::into_where_clause)
        } else {
            let mut conditions: Vec<WhereClause> = query_match
                .query_def
                .inject_params
                .iter()
                .map(|(col, source)| {
                    let value = resolve_inject_value(col, source, security_context)?;
                    Ok(inject_param_where_clause(col, value, &query_match.query_def.native_columns))
                })
                .collect::<Result<Vec<_>>>()?;

            if let Some(rls) = rls_where_clause {
                conditions.insert(0, rls.into_where_clause());
            }
            match conditions.len() {
                0 => None,
                1 => Some(conditions.remove(0)),
                _ => Some(WhereClause::And(conditions)),
            }
        };

        // 5b. Compose user-supplied WHERE from GraphQL arguments when has_where is enabled.
        //     Security conditions (RLS + inject) are always first so they cannot be bypassed.
        let combined_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            let user_where = query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?;
            match (combined_where, user_where) {
                (None, None) => None,
                (Some(sec), None) => Some(sec),
                (None, Some(user)) => Some(user),
                (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user])),
            }
        } else {
            combined_where
        };

        // 5c. Convert explicit query arguments (e.g. id, slug) to WHERE conditions.
        //     This handles single-entity lookups like `user(id: "...")` where the
        //     arguments are direct equality filters, not the structured `where` argument.
        let combined_where = combine_explicit_arg_where(
            combined_where,
            &query_match.query_def.arguments,
            &query_match.arguments,
            &query_match.query_def.native_columns,
        );

        // 8. Extract limit/offset from query arguments when auto_params are enabled
        let limit = if query_match.query_def.auto_params.has_limit {
            query_match
                .arguments
                .get("limit")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
        } else {
            None
        };

        let offset = if query_match.query_def.auto_params.has_offset {
            query_match
                .arguments
                .get("offset")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
        } else {
            None
        };

        // 8b. Extract order_by from query arguments when has_order_by is enabled,
        //     then enrich each clause with the schema field type so the SQL generator
        //     emits correct type casts (e.g., `(data->>'amount')::numeric`).
        let order_by_clauses = if query_match.query_def.auto_params.has_order_by {
            query_match
                .arguments
                .get("orderBy")
                .map(crate::db::OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        &self.schema,
                        &query_match.query_def.return_type,
                        &query_match.query_def.native_columns,
                    )
                })
        } else {
            None
        };

        // 9. Execute query with combined WHERE clause filter
        let results = self
            .adapter
            .execute_with_projection_arc(
                sql_source,
                projection_hint.as_ref(),
                combined_where.as_ref(),
                limit,
                offset,
                order_by_clauses.as_deref(),
                &session_var_refs,
            )
            .await?;

        // 10. Apply field-level RBAC filtering (reject / mask / allow)
        let access = self.apply_field_rbac_filtering(
            &query_match.query_def.return_type,
            plan.projection_fields,
            security_context,
        )?;

        // 11. Project results — include both allowed and masked fields in projection
        let mut all_projection_fields = access.allowed;
        all_projection_fields.extend(access.masked.iter().cloned());
        let projector = ResultProjector::new(all_projection_fields)
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let mut projected =
            projector.project_results(&results, query_match.query_def.returns_list)?;

        // 11. Null out masked fields in the projected result
        if !access.masked.is_empty() {
            null_masked_fields(&mut projected, &access.masked);
        }

        // 12. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 13. Store in response cache (if enabled) and return value
        if let (Some((query_key, sec_hash)), Some(rc)) =
            (response_cache_key, self.response_cache.as_ref())
        {
            let sql_source = query_match.query_def.sql_source.as_deref().unwrap_or("");
            let _ = rc.put(
                query_key,
                sec_hash,
                Arc::new(response.clone()),
                vec![sql_source.to_string()],
            );
        }

        Ok(response)
    }

    /// Compute a response cache key from a query match.
    ///
    /// Hashes the query name, matched fields, and arguments to produce
    /// a u64 key. Combined with the security context hash, this forms
    /// the full response cache key.
    fn compute_response_cache_key(query_match: &crate::runtime::matcher::QueryMatch) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = ahash::AHasher::default();
        query_match.query_def.name.hash(&mut hasher);
        for field in &query_match.fields {
            field.hash(&mut hasher);
        }
        // Hash arguments (sorted keys for determinism)
        let mut keys: Vec<&String> = query_match.arguments.keys().collect();
        keys.sort();
        for key in keys {
            key.hash(&mut hasher);
            serde_json::to_string(&query_match.arguments[key])
                .unwrap_or_default()
                .hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Execute a regular (non-aggregate, non-relay) GraphQL query.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the query does not match a compiled
    /// template or requires a security context that is not present.
    /// Returns [`FraiseQLError::Database`] if the SQL execution or result projection fails.
    pub(super) async fn execute_regular_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // 1. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // Guard: role-restricted queries are invisible to unauthenticated users
        if query_match.query_def.requires_role.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!("Query '{}' not found in schema", query_match.query_def.name),
                path:    None,
            });
        }

        // Guard: queries with inject params require a security context.
        if !query_match.query_def.inject_params.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_match.query_def.name
                ),
                path:    None,
            });
        }

        // Route relay queries to dedicated handler.
        if query_match.query_def.relay {
            return self.execute_relay_query(&query_match, variables, None).await;
        }

        // 2. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 3. Execute SQL query
        let sql_source = query_match.query_def.sql_source.as_ref().ok_or_else(|| {
            crate::error::FraiseQLError::Validation {
                message: "Query has no SQL source".to_string(),
                path:    None,
            }
        })?;

        // 3a. Generate SQL projection hint for requested fields (optimization).
        //     Recursive typed projection: composite sub-fields are projected with nested
        //     jsonb_build_object instead of returning the full blob.
        let projection_hint = if !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let root_fields = query_match
                .selections
                .first()
                .map_or(&[] as &[_], |s| s.nested_fields.as_slice());
            let typed_fields = build_typed_projection_fields(
                root_fields,
                &self.schema,
                &query_match.query_def.return_type,
                0,
            );
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_typed_projection_sql(&typed_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint {
                database:                    self.adapter.database_type(),
                projection_template:         projection_sql,
                estimated_reduction_percent: compute_projection_reduction(
                    plan.projection_fields.len(),
                ),
            })
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        // 3b. Extract auto_params (limit, offset, where, order_by) from arguments
        let user_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // 3c. Convert explicit query arguments (e.g. id, slug) to WHERE conditions.
        let user_where = combine_explicit_arg_where(
            user_where,
            &query_match.query_def.arguments,
            &query_match.arguments,
            &query_match.query_def.native_columns,
        );

        let limit = if query_match.query_def.auto_params.has_limit {
            query_match
                .arguments
                .get("limit")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
        } else {
            None
        };

        let offset = if query_match.query_def.auto_params.has_offset {
            query_match
                .arguments
                .get("offset")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
        } else {
            None
        };

        let order_by_clauses = if query_match.query_def.auto_params.has_order_by {
            query_match
                .arguments
                .get("orderBy")
                .map(crate::db::OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        &self.schema,
                        &query_match.query_def.return_type,
                        &query_match.query_def.native_columns,
                    )
                })
        } else {
            None
        };

        let results = self
            .adapter
            .execute_with_projection_arc(
                sql_source,
                projection_hint.as_ref(),
                user_where.as_ref(),
                limit,
                offset,
                order_by_clauses.as_deref(),
                &[],
            )
            .await?;

        // 4. Project results
        let projector = ResultProjector::new(plan.projection_fields)
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // 5. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 6. Serialize to JSON string
        Ok(response)
    }

    /// Execute a pre-built `QueryMatch` directly, bypassing GraphQL string parsing.
    ///
    /// Used by the REST transport for embedded sub-queries and NDJSON streaming
    /// where the query parameters are already resolved from HTTP request parameters.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the query has no SQL source.
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub async fn execute_query_direct(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        _variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // Evaluate RLS policy if present.
        let rls_where_clause: Option<RlsWhereClause> = if let (Some(ref rls_policy), Some(ctx)) =
            (&self.config.rls_policy, security_context)
        {
            rls_policy.evaluate(ctx, &query_match.query_def.name)?
        } else {
            None
        };

        // Get SQL source.
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // Build execution plan.
        let plan = self.planner.plan(query_match)?;

        // Extract auto_params from arguments.
        let user_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        let limit = query_match
            .arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok());

        let offset = query_match
            .arguments
            .get("offset")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok());

        let order_by_clauses = query_match
            .arguments
            .get("orderBy")
            .map(crate::db::OrderByClause::from_graphql_json)
            .transpose()?
            .map(|clauses| {
                enrich_order_by_clauses(
                    clauses,
                    &self.schema,
                    &query_match.query_def.return_type,
                    &query_match.query_def.native_columns,
                )
            });

        // Convert explicit arguments to WHERE conditions.
        let user_where = combine_explicit_arg_where(
            user_where,
            &query_match.query_def.arguments,
            &query_match.arguments,
            &query_match.query_def.native_columns,
        );

        // Compose RLS and user WHERE clauses.
        let composed_where = match (&rls_where_clause, &user_where) {
            (Some(rls), Some(user)) => {
                Some(WhereClause::And(vec![rls.as_where_clause().clone(), user.clone()]))
            },
            (Some(rls), None) => Some(rls.as_where_clause().clone()),
            (None, Some(user)) => Some(user.clone()),
            (None, None) => None,
        };

        // Inject security-derived params.
        if !query_match.query_def.inject_params.is_empty() {
            if let Some(ctx) = security_context {
                for (param_name, source) in &query_match.query_def.inject_params {
                    let _value = resolve_inject_value(param_name, source, ctx)?;
                    // Injected params are applied at the SQL level via WHERE clauses,
                    // not via GraphQL variables, so no mutation of variables is needed here.
                }
            }
        }

        // Execute.
        let results = self
            .adapter
            .execute_with_projection_arc(
                sql_source,
                None,
                composed_where.as_ref(),
                limit,
                offset,
                order_by_clauses.as_deref(),
                &[],
            )
            .await?;

        // Project results.
        let projector = ResultProjector::new(plan.projection_fields)
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // Wrap in GraphQL data envelope.
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        Ok(response)
    }

    /// Execute a mutation with security context for REST transport.
    ///
    /// Delegates to the standard mutation execution path with RLS enforcement.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    /// Returns `FraiseQLError::Validation` if inject params require a missing security context.
    pub async fn execute_mutation_with_security(
        &self,
        mutation_name: &str,
        arguments: &serde_json::Value,
        security_context: Option<&crate::security::SecurityContext>,
    ) -> crate::error::Result<serde_json::Value> {
        // Build a synthetic GraphQL mutation query and delegate to execute()
        let args_str = if let Some(obj) = arguments.as_object() {
            obj.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", ")
        } else {
            String::new()
        };
        let query = if args_str.is_empty() {
            format!("mutation {{ {mutation_name} {{ status entity_id message }} }}")
        } else {
            format!("mutation {{ {mutation_name}({args_str}) {{ status entity_id message }} }}")
        };

        if let Some(ctx) = security_context {
            self.execute_with_security(&query, None, ctx).await
        } else {
            self.execute(&query, None).await
        }
    }

    /// Execute a batch of mutations (for REST bulk insert).
    ///
    /// Executes each mutation individually and collects results into a `BulkResult`.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered during batch execution.
    pub async fn execute_mutation_batch(
        &self,
        mutation_name: &str,
        items: &[serde_json::Value],
        security_context: Option<&crate::security::SecurityContext>,
    ) -> crate::error::Result<crate::runtime::BulkResult> {
        let mut entities = Vec::with_capacity(items.len());
        for item in items {
            let result = self
                .execute_mutation_with_security(mutation_name, item, security_context)
                .await?;
            entities.push(result);
        }
        Ok(crate::runtime::BulkResult {
            affected_rows: entities.len() as u64,
            entities:      Some(entities),
        })
    }

    /// Execute a bulk operation (collection-level PATCH/DELETE) by filter.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub async fn execute_bulk_by_filter(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        mutation_name: &str,
        body: Option<&serde_json::Value>,
        _id_field: &str,
        _max_affected: u64,
        security_context: Option<&SecurityContext>,
    ) -> crate::error::Result<crate::runtime::BulkResult> {
        // Execute the filter query to find matching rows.
        let filter_result = self.execute_query_direct(query_match, None, security_context).await?;

        let args = body.cloned().unwrap_or(serde_json::json!({}));
        let result = self
            .execute_mutation_with_security(mutation_name, &args, security_context)
            .await?;

        let count = filter_result
            .get("data")
            .and_then(|d| d.as_object())
            .and_then(|o| o.values().next())
            .and_then(|v| v.as_array())
            .map_or(1, |a| a.len() as u64);

        Ok(crate::runtime::BulkResult {
            affected_rows: count,
            entities:      Some(vec![result]),
        })
    }

    /// Count the total number of rows matching the query's WHERE and RLS conditions.
    ///
    /// Issues a `SELECT COUNT(*) FROM {view} WHERE {conditions}` query, ignoring
    /// pagination (ORDER BY, LIMIT, OFFSET). Useful for REST `X-Total-Count` headers
    /// and `count=exact` query parameter support.
    ///
    /// # Arguments
    ///
    /// * `query_match` - Pre-built query match identifying the SQL source and filters
    /// * `variables` - Optional variables (unused for count, reserved for future use)
    /// * `security_context` - Optional authenticated user context for RLS and inject
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the query has no SQL source, or if
    /// inject params are required but no security context is provided.
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub async fn count_rows(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        _variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<u64> {
        // 1. Evaluate RLS policy
        let rls_where_clause: Option<RlsWhereClause> = if let (Some(ref rls_policy), Some(ctx)) =
            (&self.config.rls_policy, security_context)
        {
            rls_policy.evaluate(ctx, &query_match.query_def.name)?
        } else {
            None
        };

        // 2. Get SQL source
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // 3. Build combined WHERE clause (RLS + inject)
        let combined_where: Option<WhereClause> = if query_match.query_def.inject_params.is_empty()
        {
            rls_where_clause.map(RlsWhereClause::into_where_clause)
        } else {
            let ctx = security_context.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but no security context is available",
                    query_match.query_def.name
                ),
                path:    None,
            })?;
            let mut conditions: Vec<WhereClause> = query_match
                .query_def
                .inject_params
                .iter()
                .map(|(col, source)| {
                    let value = resolve_inject_value(col, source, ctx)?;
                    Ok(inject_param_where_clause(col, value, &query_match.query_def.native_columns))
                })
                .collect::<Result<Vec<_>>>()?;

            if let Some(rls) = rls_where_clause {
                conditions.insert(0, rls.into_where_clause());
            }
            match conditions.len() {
                0 => None,
                1 => Some(conditions.remove(0)),
                _ => Some(WhereClause::And(conditions)),
            }
        };

        // 3b. Compose user-supplied WHERE when has_where is enabled (same as execute_from_match).
        let combined_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            let user_where = query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?;
            match (combined_where, user_where) {
                (None, None) => None,
                (Some(sec), None) => Some(sec),
                (None, Some(user)) => Some(user),
                (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user])),
            }
        } else {
            combined_where
        };

        // 4. Execute COUNT query via adapter
        let rows = self
            .adapter
            .execute_where_query_arc(sql_source, combined_where.as_ref(), None, None, None, &[])
            .await?;

        // Return the row count
        #[allow(clippy::cast_possible_truncation)] // Reason: row count fits u64
        Ok(rows.len() as u64)
    }

    /// Execute a Relay connection query with cursor-based (keyset) pagination.
    ///
    /// Reads `first`, `after`, `last`, `before` from `variables`, fetches a page
    /// of rows using `pk_{type}` keyset ordering, and wraps the result in the
    /// Relay `XxxConnection` format:
    /// ```json
    /// {
    ///   "data": {
    ///     "users": {
    ///       "edges": [{ "cursor": "NDI=", "node": { "id": "...", ... } }],
    ///       "pageInfo": {
    ///         "hasNextPage": true, "hasPreviousPage": false,
    ///         "startCursor": "NDI=", "endCursor": "Mw=="
    ///       }
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if required pagination variables are
    /// missing or contain invalid cursor values.
    /// Returns [`FraiseQLError::Database`] if the SQL execution or result projection fails.
    pub(super) async fn execute_relay_query(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        use crate::{
            compiler::aggregation::OrderByClause,
            runtime::relay::{decode_edge_cursor, decode_uuid_cursor, encode_edge_cursor},
            schema::CursorType,
        };

        let query_def = &query_match.query_def;

        // Guard: queries with inject params require a security context.
        if !query_def.inject_params.is_empty() && security_context.is_none() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_def.name
                ),
                path:    None,
            });
        }

        let sql_source =
            query_def.sql_source.as_deref().ok_or_else(|| FraiseQLError::Validation {
                message: format!("Relay query '{}' has no sql_source configured", query_def.name),
                path:    None,
            })?;

        let cursor_column =
            query_def
                .relay_cursor_column
                .as_deref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!(
                        "Relay query '{}' has no relay_cursor_column derived",
                        query_def.name
                    ),
                    path:    None,
                })?;

        // Resolve session variables once for all relay query methods.
        let session_vars: Vec<(String, String)> = {
            let sv = &self.schema.session_variables;
            if !sv.variables.is_empty() || sv.inject_started_at {
                if let Some(sc) = security_context {
                    let vars = crate::runtime::executor::security::resolve_session_variables(
                        sv,
                        sc,
                    );
                    vars.into_iter().collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        };
        let session_var_refs: Vec<(&str, &str)> = session_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        // Guard: relay pagination requires the executor to have been constructed
        // via `Executor::new_with_relay` with a `RelayDatabaseAdapter`.
        let relay = self.relay.as_ref().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Relay pagination is not supported by the {} adapter. \
                  Use a relay-capable adapter (e.g. PostgreSQL) and construct \
                  the executor with `Executor::new_with_relay`.",

                self.adapter.database_type(),
            ),
            path:    None,
        })?;

        // --- RLS + inject_params evaluation (same logic as execute_from_match) ---
        // Evaluate RLS policy to generate security WHERE clause.
        let rls_where_clause: Option<RlsWhereClause> = if let (Some(ref rls_policy), Some(ctx)) =
            (&self.config.rls_policy, security_context)
        {
            rls_policy.evaluate(ctx, &query_def.name)?
        } else {
            None
        };

        // Resolve inject_params from JWT claims and compose with RLS.
        let security_where: Option<WhereClause> = if query_def.inject_params.is_empty() {
            rls_where_clause.map(RlsWhereClause::into_where_clause)
        } else {
            let ctx = security_context.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_def.name
                ),
                path:    None,
            })?;
            let mut conditions: Vec<WhereClause> = query_def
                .inject_params
                .iter()
                .map(|(col, source)| {
                    let value = resolve_inject_value(col, source, ctx)?;
                    Ok(inject_param_where_clause(col, value, &query_def.native_columns))
                })
                .collect::<Result<Vec<_>>>()?;

            if let Some(rls) = rls_where_clause {
                conditions.insert(0, rls.into_where_clause());
            }
            match conditions.len() {
                0 => None,
                1 => Some(conditions.remove(0)),
                _ => Some(WhereClause::And(conditions)),
            }
        };

        // Extract relay pagination arguments from variables.
        let vars = variables.and_then(|v| v.as_object());
        let first: Option<u32> = vars
            .and_then(|v| v.get("first"))
            .and_then(|v| v.as_u64())
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        let last: Option<u32> = vars
            .and_then(|v| v.get("last"))
            .and_then(|v| v.as_u64())
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        let after_cursor: Option<&str> = vars.and_then(|v| v.get("after")).and_then(|v| v.as_str());
        let before_cursor: Option<&str> =
            vars.and_then(|v| v.get("before")).and_then(|v| v.as_str());

        // Decode base64 cursors — type depends on relay_cursor_type.
        // If a cursor string is provided but fails to decode, return a validation
        // error immediately. Silently ignoring an invalid cursor would return a
        // full result set, violating the client's pagination intent.
        let (after_pk, before_pk) =
            match query_def.relay_cursor_type {
                CursorType::Int64 => {
                    let after = match after_cursor {
                        Some(s) => Some(decode_edge_cursor(s).map(CursorValue::Int64).ok_or_else(
                            || FraiseQLError::Validation {
                                message: format!("invalid relay cursor for `after`: {s:?}"),
                                path:    Some("after".to_string()),
                            },
                        )?),
                        None => None,
                    };
                    let before = match before_cursor {
                        Some(s) => Some(decode_edge_cursor(s).map(CursorValue::Int64).ok_or_else(
                            || FraiseQLError::Validation {
                                message: format!("invalid relay cursor for `before`: {s:?}"),
                                path:    Some("before".to_string()),
                            },
                        )?),
                        None => None,
                    };
                    (after, before)
                },
                CursorType::Uuid => {
                    let after = match after_cursor {
                        Some(s) => {
                            Some(decode_uuid_cursor(s).map(CursorValue::Uuid).ok_or_else(|| {
                                FraiseQLError::Validation {
                                    message: format!("invalid relay cursor for `after`: {s:?}"),
                                    path:    Some("after".to_string()),
                                }
                            })?)
                        },
                        None => None,
                    };
                    let before = match before_cursor {
                        Some(s) => {
                            Some(decode_uuid_cursor(s).map(CursorValue::Uuid).ok_or_else(|| {
                                FraiseQLError::Validation {
                                    message: format!("invalid relay cursor for `before`: {s:?}"),
                                    path:    Some("before".to_string()),
                                }
                            })?)
                        },
                        None => None,
                    };
                    (after, before)
                },
            };

        // Determine direction and limit.
        // Forward pagination takes priority; fallback to 20 if neither first/last given.
        let (forward, page_size) = if last.is_some() && first.is_none() {
            (false, last.unwrap_or(20))
        } else {
            (true, first.unwrap_or(20))
        };

        // Fetch page_size + 1 rows to detect hasNextPage/hasPreviousPage.
        let fetch_limit = page_size + 1;

        // Parse optional `where` filter from variables.
        let user_where_clause = if query_def.auto_params.has_where {
            vars.and_then(|v| v.get("where"))
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // Compose final WHERE: security (RLS + inject) AND user-supplied WHERE.
        // Security conditions always come first so they cannot be bypassed.
        let combined_where = match (security_where, user_where_clause) {
            (None, None) => None,
            (Some(sec), None) => Some(sec),
            (None, Some(user)) => Some(user),
            (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user])),
        };

        // Parse optional `orderBy` from variables, enriched with schema type info.
        let order_by = if query_def.auto_params.has_order_by {
            vars.and_then(|v| v.get("orderBy"))
                .map(OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        &self.schema,
                        &query_def.return_type,
                        &query_def.native_columns,
                    )
                })
        } else {
            None
        };

        // Detect whether the client selected `totalCount` inside the connection.
        // Named fragment spreads are already expanded by the matcher's FragmentResolver.
        // Inline fragments (`... on UserConnection { totalCount }`) remain as FieldSelection
        // entries with a name starting with "..." — we recurse one level into those.
        let include_total_count = query_match
            .selections
            .iter()
            .find(|sel| sel.name == query_def.name)
            .is_some_and(|connection_field| {
                selections_contain_field(&connection_field.nested_fields, "totalCount")
            });

        // Capture before the move into execute_relay_page.
        let had_after = after_pk.is_some();
        let had_before = before_pk.is_some();

        let result = relay
            .execute_relay_page(
                sql_source,
                cursor_column,
                after_pk,
                before_pk,
                fetch_limit,
                forward,
                combined_where.as_ref(),
                order_by.as_deref(),
                include_total_count,
                &session_var_refs,
            )
            .await?;

        // Detect whether there are more pages.
        let has_extra = result.rows.len() > page_size as usize;
        let rows: Vec<_> = result.rows.into_iter().take(page_size as usize).collect();

        let (has_next_page, has_previous_page) = if forward {
            (has_extra, had_after)
        } else {
            (had_before, has_extra)
        };

        // Build edges: each edge has { cursor, node }.
        let mut edges = Vec::with_capacity(rows.len());
        let mut start_cursor_str: Option<String> = None;
        let mut end_cursor_str: Option<String> = None;

        for (i, row) in rows.iter().enumerate() {
            let data = &row.data;

            let col_val = data.as_object().and_then(|obj| obj.get(cursor_column));

            let cursor_str = match query_def.relay_cursor_type {
                CursorType::Int64 => col_val
                    .and_then(|v| v.as_i64())
                    .map(encode_edge_cursor)
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "Relay query '{}': cursor column '{}' not found or not an integer in \
                             result JSONB. Ensure the view exposes this column inside the `data` object.",
                            query_def.name, cursor_column
                        ),
                        path: None,
                    })?,
                CursorType::Uuid => col_val
                    .and_then(|v| v.as_str())
                    .map(crate::runtime::relay::encode_uuid_cursor)
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "Relay query '{}': cursor column '{}' not found or not a string in \
                             result JSONB. Ensure the view exposes this column inside the `data` object.",
                            query_def.name, cursor_column
                        ),
                        path: None,
                    })?,
            };

            if i == 0 {
                start_cursor_str = Some(cursor_str.clone());
            }
            end_cursor_str = Some(cursor_str.clone());

            edges.push(serde_json::json!({
                "cursor": cursor_str,
                "node": data,
            }));
        }

        let page_info = serde_json::json!({
            "hasNextPage": has_next_page,
            "hasPreviousPage": has_previous_page,
            "startCursor": start_cursor_str,
            "endCursor": end_cursor_str,
        });

        let mut connection = serde_json::json!({
            "edges": edges,
            "pageInfo": page_info,
        });

        // Include totalCount when the client requested it and the adapter provided it.
        if include_total_count {
            if let Some(count) = result.total_count {
                connection["totalCount"] = serde_json::json!(count);
            } else {
                connection["totalCount"] = serde_json::Value::Null;
            }
        }

        let response = ResultProjector::wrap_in_data_envelope(connection, &query_def.name);
        Ok(response)
    }

    /// Execute a Relay global `node(id: ID!)` query.
    ///
    /// Decodes the opaque node ID (`base64("TypeName:uuid")`), locates the
    /// appropriate SQL view by searching the compiled schema for a query that
    /// returns that type, and fetches the matching row.
    ///
    /// Returns `{ "data": { "node": <object> } }` on success, or
    /// `{ "data": { "node": null } }` when the object is not found.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` when:
    /// - The `id` argument is missing or malformed
    /// - No SQL view is registered for the requested type
    pub(super) async fn execute_node_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        selections: &[FieldSelection],
    ) -> Result<serde_json::Value> {
        use crate::{
            db::{WhereClause, where_clause::WhereOperator},
            runtime::relay::decode_node_id,
        };

        // 1. Extract the raw opaque ID. Priority: $variables.id > inline literal in query text.
        let raw_id: String = if let Some(id_val) = variables
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("id"))
            .and_then(|v| v.as_str())
        {
            id_val.to_string()
        } else {
            // Fall back to extracting inline literal, e.g. node(id: "NDI=")
            Self::extract_inline_node_id(query).ok_or_else(|| FraiseQLError::Validation {
                message: "node query: missing or unresolvable 'id' argument".to_string(),
                path:    Some("node.id".to_string()),
            })?
        };

        // 2. Decode base64("TypeName:uuid") → (type_name, uuid).
        let (type_name, uuid) =
            decode_node_id(&raw_id).ok_or_else(|| FraiseQLError::Validation {
                message: format!("node query: invalid node ID '{raw_id}'"),
                path:    Some("node.id".to_string()),
            })?;

        // 3. Find the SQL view for this type (O(1) index lookup built at startup).
        let sql_source: Arc<str> =
            self.node_type_index.get(&type_name).cloned().ok_or_else(|| {
                FraiseQLError::Validation {
                    message: format!("node query: no registered SQL view for type '{type_name}'"),
                    path:    Some("node.id".to_string()),
                }
            })?;

        // 4. Build WHERE clause: data->>'id' = uuid
        let where_clause = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::Value::String(uuid),
        };

        // 5. Build projection hint from selections (mirrors regular query path).
        let projection_hint = if !selections.is_empty() {
            let typed_fields =
                build_typed_projection_fields(selections, &self.schema, &type_name, 0);
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_typed_projection_sql(&typed_fields)
                .unwrap_or_else(|_| "data".to_string());
            Some(SqlProjectionHint {
                database:                    self.adapter.database_type(),
                projection_template:         projection_sql,
                estimated_reduction_percent: compute_projection_reduction(typed_fields.len()),
            })
        } else {
            None
        };

        // 6. Execute the query (limit 1) with projection.
        let rows = self
            .adapter
            .execute_with_projection_arc(
                &sql_source,
                projection_hint.as_ref(),
                Some(&where_clause),
                Some(1),
                None,
                None,
                &[],
            )
            .await?;

        // 7. Return the first matching row (or null).
        // When the Arc is exclusively owned (uncached path, refcount = 1) we can move the
        // data out without copying.  When the cache also holds a reference (refcount ≥ 2)
        // we clone the single `serde_json::Value` for this one-row lookup.
        let node_value = Arc::try_unwrap(rows).map_or_else(
            |arc| arc.first().map_or(serde_json::Value::Null, |row| row.data.clone()),
            |v| v.into_iter().next().map_or(serde_json::Value::Null, |row| row.data),
        );

        let response = ResultProjector::wrap_in_data_envelope(node_value, "node");
        Ok(response)
    }
}

/// Estimate the payload reduction percentage from projecting N fields.
///
/// Uses a simple heuristic: each projected field saves proportional space
/// relative to a baseline of 20 typical JSONB fields per row. Clamped to
/// [10, 90] so the hint is never misleadingly extreme.
fn compute_projection_reduction(projected_field_count: usize) -> u32 {
    // Baseline: assume a typical type has 20 fields.
    const BASELINE_FIELD_COUNT: usize = 20;
    let requested = projected_field_count.min(BASELINE_FIELD_COUNT);
    let saved = BASELINE_FIELD_COUNT.saturating_sub(requested);
    // saved / BASELINE * 100, clamped to [10, 90]
    #[allow(clippy::cast_possible_truncation)] // Reason: result is in 0..=100, fits u32
    let percent = ((saved * 100) / BASELINE_FIELD_COUNT) as u32;
    percent.clamp(10, 90)
}

/// Return `true` if `field_name` appears in `selections`, including inside inline
/// fragment entries (`FieldSelection` whose name starts with `"..."`).
///
/// Named fragment spreads are already flattened by [`FragmentResolver`] before this
/// is called, so we only need to recurse one level into inline fragments.
fn selections_contain_field(
    selections: &[crate::graphql::FieldSelection],
    field_name: &str,
) -> bool {
    for sel in selections {
        if sel.name == field_name {
            return true;
        }
        // Inline fragment: name starts with "..." (e.g. "...on UserConnection")
        if sel.name.starts_with("...") && selections_contain_field(&sel.nested_fields, field_name) {
            return true;
        }
    }
    false
}

/// Auto-wired argument names that are handled by the `auto_params` system.
/// These are never treated as explicit WHERE filters.
const AUTO_PARAM_NAMES: &[&str] = &[
    "where", "limit", "offset", "orderBy", "first", "last", "after", "before",
];

/// Build a `WhereClause` for a single inject param, respecting `native_columns`.
fn inject_param_where_clause(
    col: &str,
    value: serde_json::Value,
    native_columns: &std::collections::HashMap<String, String>,
) -> WhereClause {
    if let Some(pg_type) = native_columns.get(col) {
        WhereClause::NativeField {
            column: col.to_string(),
            pg_cast: pg_type_to_cast(pg_type).to_string(),
            operator: WhereOperator::Eq,
            value,
        }
    } else {
        WhereClause::Field {
            path: vec![col.to_string()],
            operator: WhereOperator::Eq,
            value,
        }
    }
}

/// Convert PostgreSQL `information_schema.data_type` to a safe SQL cast suffix.
///
/// Returns an empty string for types that need no cast (e.g. `text`, `varchar`).
/// Normalise a database type name for use as the `pg_cast` hint in
/// `WhereClause::NativeField`.
///
/// The returned string is the **canonical PostgreSQL type name** (e.g. `"uuid"`,
/// `"int4"`, `"timestamp"`).  It is passed to `SqlDialect::cast_native_param`
/// which translates it into the dialect-appropriate cast expression:
/// - PostgreSQL: `$1::text::uuid`  (two-step to avoid binary wire-format mismatch)
/// - MySQL:      `CAST(? AS CHAR)`
/// - SQLite:     `CAST(? AS TEXT)`
/// - SQL Server: `CAST(@p1 AS UNIQUEIDENTIFIER)`
///
/// Returns `""` for text-like types that need no cast.
fn pg_type_to_cast(data_type: &str) -> &'static str {
    crate::runtime::native_columns::pg_type_to_cast(data_type)
}

/// Convert explicit query arguments (e.g. `id`, `slug`, `email`) into
/// WHERE equality conditions and AND them onto `existing`.
///
/// Arguments whose names match auto-wired parameters (`where`, `limit`,
/// `offset`, `orderBy`, `first`, `last`, `after`, `before`) are skipped —
/// they are handled separately by the auto-params system.
///
/// When an argument has a matching entry in `native_columns`, a
/// `WhereClause::NativeField` is emitted (enabling B-tree index lookup via
/// `WHERE col = $N::type`).  Otherwise a `WhereClause::Field` is emitted
/// (JSONB extraction: `WHERE data->>'col' = $N`).
fn combine_explicit_arg_where(
    existing: Option<WhereClause>,
    defined_args: &[crate::schema::ArgumentDefinition],
    provided_args: &std::collections::HashMap<String, serde_json::Value>,
    native_columns: &std::collections::HashMap<String, String>,
) -> Option<WhereClause> {
    let explicit_conditions: Vec<WhereClause> = defined_args
        .iter()
        .filter(|arg| !AUTO_PARAM_NAMES.contains(&arg.name.as_str()))
        .filter_map(|arg| {
            provided_args.get(&arg.name).map(|value| {
                if let Some(pg_type) = native_columns.get(&arg.name) {
                    WhereClause::NativeField {
                        column:   arg.name.clone(),
                        pg_cast:  pg_type_to_cast(pg_type).to_string(),
                        operator: WhereOperator::Eq,
                        value:    value.clone(),
                    }
                } else {
                    WhereClause::Field {
                        path:     vec![arg.name.clone()],
                        operator: WhereOperator::Eq,
                        value:    value.clone(),
                    }
                }
            })
        })
        .collect();

    if explicit_conditions.is_empty() {
        return existing;
    }

    let mut all_conditions = Vec::new();
    if let Some(prev) = existing {
        all_conditions.push(prev);
    }
    all_conditions.extend(explicit_conditions);

    match all_conditions.len() {
        1 => Some(all_conditions.remove(0)),
        _ => Some(WhereClause::And(all_conditions)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql::FieldSelection;

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn leaf(name: &str) -> FieldSelection {
        FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }
    }

    fn fragment(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
        FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: nested,
            directives:    vec![],
        }
    }

    // =========================================================================
    // compute_projection_reduction
    // =========================================================================

    #[test]
    fn projection_reduction_zero_fields_is_clamped_to_90() {
        // 0 fields requested → saved = 20 → 100% → clamped to 90
        assert_eq!(compute_projection_reduction(0), 90);
    }

    #[test]
    fn projection_reduction_all_fields_is_clamped_to_10() {
        // 20 fields (= baseline) → saved = 0 → 0% → clamped to 10
        assert_eq!(compute_projection_reduction(20), 10);
    }

    #[test]
    fn projection_reduction_above_baseline_clamps_to_10() {
        // 50 fields > 20 baseline → same as 20 → clamped to 10
        assert_eq!(compute_projection_reduction(50), 10);
    }

    #[test]
    fn projection_reduction_10_fields_is_50_percent() {
        // 10 requested → saved = 10 → 10/20 * 100 = 50 → within [10, 90]
        assert_eq!(compute_projection_reduction(10), 50);
    }

    #[test]
    fn projection_reduction_1_field_is_high() {
        // 1 requested → saved = 19 → 95% → clamped to 90
        assert_eq!(compute_projection_reduction(1), 90);
    }

    #[test]
    fn projection_reduction_result_always_in_clamp_range() {
        for n in 0_usize..=30 {
            let r = compute_projection_reduction(n);
            assert!((10..=90).contains(&r), "out of [10,90] for n={n}: got {r}");
        }
    }

    // =========================================================================
    // selections_contain_field
    // =========================================================================

    #[test]
    fn empty_selections_returns_false() {
        assert!(!selections_contain_field(&[], "totalCount"));
    }

    #[test]
    fn direct_match_returns_true() {
        let sels = vec![leaf("edges"), leaf("totalCount"), leaf("pageInfo")];
        assert!(selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn absent_field_returns_false() {
        let sels = vec![leaf("edges"), leaf("pageInfo")];
        assert!(!selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn inline_fragment_nested_match_returns_true() {
        // "...on UserConnection" wrapping totalCount
        let inline = fragment("...on UserConnection", vec![leaf("totalCount"), leaf("edges")]);
        let sels = vec![inline];
        assert!(selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn inline_fragment_does_not_spuriously_match_fragment_name() {
        // The fragment entry (name "...on Foo") only matches a field named exactly "...on Foo"
        // when searched directly; it should NOT match an unrelated field name.
        let inline = fragment("...on Foo", vec![leaf("id")]);
        let sels = vec![inline];
        assert!(!selections_contain_field(&sels, "totalCount"));
        // "id" is nested inside the fragment and should be found via recursion
        assert!(selections_contain_field(&sels, "id"));
    }

    #[test]
    fn field_not_in_fragment_returns_false() {
        let inline = fragment("...on UserConnection", vec![leaf("edges"), leaf("pageInfo")]);
        let sels = vec![inline];
        assert!(!selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn non_fragment_nested_field_not_searched() {
        // Only entries whose name starts with "..." trigger recursion.
        // A plain field's nested_fields should NOT be recursed into.
        let nested_count = fragment("edges", vec![leaf("totalCount")]);
        let sels = vec![nested_count];
        // "edges" doesn't start with "..." — nested fields not searched
        assert!(!selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn multiple_fragments_any_can_match() {
        let frag1 = fragment("...on TypeA", vec![leaf("id")]);
        let frag2 = fragment("...on TypeB", vec![leaf("totalCount")]);
        let sels = vec![frag1, frag2];
        assert!(selections_contain_field(&sels, "totalCount"));
        assert!(selections_contain_field(&sels, "id"));
        assert!(!selections_contain_field(&sels, "name"));
    }

    #[test]
    fn mixed_direct_and_fragment_selections() {
        let inline = fragment("...on Connection", vec![leaf("pageInfo")]);
        let sels = vec![leaf("edges"), inline, leaf("metadata")];
        assert!(selections_contain_field(&sels, "edges"));
        assert!(selections_contain_field(&sels, "pageInfo"));
        assert!(selections_contain_field(&sels, "metadata"));
        assert!(!selections_contain_field(&sels, "cursor"));
    }

    // =========================================================================
    // combine_explicit_arg_where
    // =========================================================================

    use crate::schema::{ArgumentDefinition, FieldType};

    fn make_arg(name: &str) -> ArgumentDefinition {
        ArgumentDefinition::new(name, FieldType::Id)
    }

    #[test]
    fn no_explicit_args_returns_existing() {
        let existing = Some(WhereClause::Field {
            path:     vec!["rls".into()],
            operator: WhereOperator::Eq,
            value:    serde_json::json!("x"),
        });
        let result = combine_explicit_arg_where(
            existing.clone(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert_eq!(result, existing);
    }

    #[test]
    fn explicit_id_arg_produces_where_clause() {
        let args = vec![make_arg("id")];
        let mut provided = std::collections::HashMap::new();
        provided.insert("id".into(), serde_json::json!("uuid-123"));

        let result =
            combine_explicit_arg_where(None, &args, &provided, &std::collections::HashMap::new());
        assert!(result.is_some(), "explicit id arg should produce a WHERE clause");
        match result.expect("just asserted Some") {
            WhereClause::Field {
                path,
                operator,
                value,
            } => {
                assert_eq!(path, vec!["id".to_string()]);
                assert_eq!(operator, WhereOperator::Eq);
                assert_eq!(value, serde_json::json!("uuid-123"));
            },
            other => panic!("expected Field, got {other:?}"),
        }
    }

    #[test]
    fn auto_param_names_are_skipped() {
        let args = vec![
            make_arg("where"),
            make_arg("limit"),
            make_arg("offset"),
            make_arg("orderBy"),
            make_arg("first"),
            make_arg("last"),
            make_arg("after"),
            make_arg("before"),
            make_arg("id"),
        ];
        let mut provided = std::collections::HashMap::new();
        for name in &[
            "where", "limit", "offset", "orderBy", "first", "last", "after", "before", "id",
        ] {
            provided.insert((*name).to_string(), serde_json::json!("value"));
        }

        let result =
            combine_explicit_arg_where(None, &args, &provided, &std::collections::HashMap::new());
        // Only "id" should produce a WHERE — all auto-param names are skipped
        match result.expect("id arg should produce WHERE") {
            WhereClause::Field { path, .. } => {
                assert_eq!(path, vec!["id".to_string()]);
            },
            other => panic!("expected single Field for 'id', got {other:?}"),
        }
    }

    #[test]
    fn explicit_args_combined_with_existing_where() {
        let existing = WhereClause::Field {
            path:     vec!["rls_tenant".into()],
            operator: WhereOperator::Eq,
            value:    serde_json::json!("tenant-1"),
        };
        let args = vec![make_arg("id")];
        let mut provided = std::collections::HashMap::new();
        provided.insert("id".into(), serde_json::json!("uuid-456"));

        let result = combine_explicit_arg_where(
            Some(existing),
            &args,
            &provided,
            &std::collections::HashMap::new(),
        );
        match result.expect("should produce combined WHERE") {
            WhereClause::And(conditions) => {
                assert_eq!(conditions.len(), 2, "should AND existing + explicit");
            },
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn unprovided_explicit_arg_is_ignored() {
        let args = vec![make_arg("id"), make_arg("slug")];
        let mut provided = std::collections::HashMap::new();
        // Only provide "id", not "slug"
        provided.insert("id".into(), serde_json::json!("uuid-789"));

        let result =
            combine_explicit_arg_where(None, &args, &provided, &std::collections::HashMap::new());
        match result.expect("id arg should produce WHERE") {
            WhereClause::Field { path, .. } => {
                assert_eq!(path, vec!["id".to_string()]);
            },
            other => panic!("expected single Field for 'id', got {other:?}"),
        }
    }

    // =========================================================================
    // pg_type_to_cast — returns canonical type names passed to SqlDialect::cast_native_param
    // =========================================================================

    #[test]
    fn uuid_normalises_to_canonical_type_name() {
        assert_eq!(pg_type_to_cast("uuid"), "uuid");
        assert_eq!(pg_type_to_cast("UUID"), "uuid");
    }

    #[test]
    fn integer_types_normalise_to_canonical_names() {
        assert_eq!(pg_type_to_cast("integer"), "int4");
        assert_eq!(pg_type_to_cast("int4"), "int4");
        assert_eq!(pg_type_to_cast("bigint"), "int8");
        assert_eq!(pg_type_to_cast("int8"), "int8");
        assert_eq!(pg_type_to_cast("smallint"), "int2");
        assert_eq!(pg_type_to_cast("int2"), "int2");
    }

    #[test]
    fn float_and_numeric_types_normalise_to_canonical_names() {
        assert_eq!(pg_type_to_cast("numeric"), "numeric");
        assert_eq!(pg_type_to_cast("decimal"), "numeric");
        assert_eq!(pg_type_to_cast("double precision"), "float8");
        assert_eq!(pg_type_to_cast("float8"), "float8");
        assert_eq!(pg_type_to_cast("real"), "float4");
        assert_eq!(pg_type_to_cast("float4"), "float4");
    }

    #[test]
    fn date_and_time_types_normalise_to_canonical_names() {
        assert_eq!(pg_type_to_cast("timestamp"), "timestamp");
        assert_eq!(pg_type_to_cast("timestamp without time zone"), "timestamp");
        assert_eq!(pg_type_to_cast("timestamptz"), "timestamptz");
        assert_eq!(pg_type_to_cast("timestamp with time zone"), "timestamptz");
        assert_eq!(pg_type_to_cast("date"), "date");
        assert_eq!(pg_type_to_cast("time"), "time");
        assert_eq!(pg_type_to_cast("time without time zone"), "time");
    }

    #[test]
    fn bool_normalises_to_canonical_name() {
        assert_eq!(pg_type_to_cast("boolean"), "bool");
        assert_eq!(pg_type_to_cast("bool"), "bool");
    }

    #[test]
    fn text_types_produce_empty_hint_meaning_no_cast() {
        assert_eq!(pg_type_to_cast("text"), "");
        assert_eq!(pg_type_to_cast("varchar"), "");
        assert_eq!(pg_type_to_cast("unknown_type"), "");
    }
}
